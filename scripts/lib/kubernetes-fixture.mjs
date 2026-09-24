import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { labels, security, secretKeys } from "./kubernetes-pods.mjs";
const database =
  "percona/percona-distribution-postgresql:18.6@sha256:dae47360e8137cafc1e8d66f9a1be348f1405e3cf51daa383b94e6c277e6b256";
const redis =
  "redis:8.10.1@sha256:298e5b3bc566bade82f46ad5511777a4a07a294097ce16ada2f6a42be5239df5";
export async function backendFixture(c, directory, ingressPolicies) {
  const namespace = c.backendNamespace;
  const object = (kind, name, fields) => ({
    apiVersion: "v1",
    kind,
    metadata: { name, namespace },
    ...fields,
  });
  const secret = async (name, files) =>
    object("Secret", name, {
      type: "Opaque",
      data: Object.fromEntries(
        await Promise.all(
          files.map(async ([key, file]) => [
            key,
            (await readFile(join(directory, "secrets", file))).toString(
              "base64",
            ),
          ]),
        ),
      ),
    });
  const appSecret = async (name, keys) => ({
    ...(await secret(
      name,
      keys.map((f) => [f === "ca.pem" ? "ca" : f, f]),
    )),
    metadata: { name, namespace: c.namespace },
  });
  const configs = object("ConfigMap", "fixture-configuration", {
    data: {
      "initialize.sql": await readFile("deploy/initialize.sql", "utf8"),
      "pg_hba.conf": await readFile("deploy/pg_hba.conf", "utf8"),
      "postgres-entrypoint.sh": (
        await readFile("deploy/postgres-entrypoint.sh", "utf8")
      ).replaceAll("/run/darkhorse", "/run/darkhorse/tls"),
      "cache.conf": await readFile("deploy/cache.conf", "utf8"),
      "limiter.conf": await readFile("deploy/limiter.conf", "utf8"),
    },
  });
  const items = [
    { apiVersion: "v1", kind: "Namespace", metadata: { name: namespace } },
    {
      apiVersion: "v1",
      kind: "Namespace",
      metadata: { name: c.ingressNamespace },
    },
    ...ingressPolicies,
    configs,
    ...(await Promise.all(
      ["runtime", "operator", "migrator"].map((role) =>
        appSecret(
          `darkhorse-${role}-secrets`,
          secretKeys(role).map((key) => (key === "ca" ? "ca.pem" : key)),
        ),
      ),
    )),
    {
      ...(await secret("darkhorse-edge-tls", [
        ["tls.crt", "edge.pem"],
        ["tls.key", "edge.key"],
        ["ca", "ca.pem"],
      ])),
      metadata: { name: "darkhorse-edge-tls", namespace: c.namespace },
    },
    await secret("postgres", [
      ["postgres_password", "postgres-password"],
      ["owner_password", "owner-password"],
      ["operator_password", "operator-password"],
      ["runtime_password", "runtime-password"],
      ["database_cert", "postgres.pem"],
      ["database_key", "postgres.key"],
    ]),
    ...(await Promise.all(
      ["cache", "limiter"].map((role) =>
        secret(role, [
          [`${role}_cert`, `${role}.pem`],
          [`${role}_key`, `${role}.key`],
          [`${role}_acl`, `${role}-acl`],
          ["ca", "ca.pem"],
        ]),
      ),
    )),
  ];
  for (const role of ["postgres", "cache", "limiter"]) {
    const postgres = role === "postgres",
      uid = postgres ? 26 : 999;
    const container = {
      name: role,
      image: postgres ? database : redis,
      imagePullPolicy: "IfNotPresent",
      securityContext: {
        ...security(),
        runAsUser: uid,
        runAsGroup: uid,
        readOnlyRootFilesystem: !postgres,
      },
      resources: {
        requests: { cpu: "100m", memory: "128Mi" },
        limits: { cpu: "1", memory: "512Mi" },
      },
      volumeMounts: [
        { name: "secrets", mountPath: "/run/secrets", readOnly: true },
        { name: "data", mountPath: "/data" },
        {
          name: "settings",
          mountPath: postgres ? "/etc/darkhorse" : "/etc/redis",
          readOnly: true,
        },
        ...(postgres
          ? [
              { name: "runtime", mountPath: "/run/darkhorse" },
              {
                name: "init",
                mountPath: "/docker-entrypoint-initdb.d",
                readOnly: true,
              },
            ]
          : []),
      ],
      ports: [{ containerPort: postgres ? 5432 : 6379 }],
    };
    if (postgres) {
      // Keep the postmaster outside PID 1 so fault tests can stop it and all workers.
      container.command = [
        "sh",
        "-c",
        'sh /etc/darkhorse/postgres-entrypoint.sh & database_pid=$!; trap \'kill -TERM "$database_pid"; wait "$database_pid"\' TERM INT; wait "$database_pid"',
      ];
      container.env = [
        { name: "PGDATA", value: "/data/db" },
        { name: "POSTGRES_USER", value: "postgres" },
        { name: "POSTGRES_DB", value: "postgres" },
        {
          name: "POSTGRES_PASSWORD_FILE",
          value: "/run/secrets/postgres_password",
        },
        { name: "POSTGRES_INITDB_ARGS", value: "--encoding=UTF8" },
      ];
      container.readinessProbe = {
        exec: {
          command: [
            "psql",
            "-U",
            "postgres",
            "-d",
            "darkhorse",
            "-tAc",
            "SELECT 1",
          ],
        },
        periodSeconds: 2,
        timeoutSeconds: 2,
      };
    } else container.args = ["redis-server", `/etc/redis/${role}.conf`];
    items.push(
      object("Pod", role, {
        metadata: { name: role, namespace, labels: labels(role) },
        spec: {
          automountServiceAccountToken: false,
          securityContext: { fsGroup: uid },
          containers: [container],
          volumes: [
            { name: "secrets", secret: { secretName: role } },
            { name: "data", emptyDir: {} },
            { name: "settings", configMap: { name: "fixture-configuration" } },
            ...(postgres
              ? [
                  { name: "runtime", emptyDir: {} },
                  {
                    name: "init",
                    configMap: {
                      name: "fixture-configuration",
                      items: [
                        { key: "initialize.sql", path: "01-darkhorse.sql" },
                      ],
                    },
                  },
                ]
              : []),
          ],
        },
      }),
    );
    items.push(
      object("Service", role, {
        spec: {
          selector: labels(role),
          ports: [{ port: postgres ? 5432 : 6379 }],
        },
      }),
    );
    items.push({
      apiVersion: "v1",
      kind: "Service",
      metadata: { name: role, namespace: c.namespace },
      spec: {
        type: "ExternalName",
        externalName: `${role}.${namespace}.svc.cluster.local`,
      },
    });
  }
  return { apiVersion: "v1", kind: "List", items };
}
export function clientFixture(c, namespace) {
  return {
    apiVersion: "v1",
    kind: "Pod",
    metadata: { name: "probe", namespace },
    spec: {
      automountServiceAccountToken: false,
      securityContext: {
        runAsNonRoot: true,
        runAsUser: 10001,
        runAsGroup: 10001,
        fsGroup: 10001,
        seccompProfile: { type: "RuntimeDefault" },
      },
      containers: [
        {
          name: "probe",
          image: c.edgeImage,
          imagePullPolicy: "IfNotPresent",
          command: ["/bin/sleep", "1200"],
          securityContext: security(),
          resources: {
            requests: { cpu: "10m", memory: "16Mi" },
            limits: { cpu: "100m", memory: "64Mi" },
          },
          volumeMounts: [
            { name: "trust", mountPath: "/run/trust", readOnly: true },
          ],
        },
      ],
      volumes: [
        {
          name: "trust",
          secret: { secretName: "probe-trust", defaultMode: 0o440 },
        },
      ],
    },
  };
}
