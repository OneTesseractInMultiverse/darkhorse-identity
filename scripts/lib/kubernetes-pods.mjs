export const labels = (component) => ({
  "app.kubernetes.io/name": "darkhorse",
  "app.kubernetes.io/component": component,
});
export const security = () => ({
  runAsNonRoot: true,
  runAsUser: 10001,
  runAsGroup: 10001,
  readOnlyRootFilesystem: true,
  allowPrivilegeEscalation: false,
  capabilities: { drop: ["ALL"] },
  seccompProfile: { type: "RuntimeDefault" },
});
const env = (name, value) => ({ name, value });
export function secretKeys(role) {
  if (role === "migrator") return ["owner-db", "ca"];
  if (role === "account")
    return ["runtime-db", "login-key", "cache-url", "limiter-url", "ca"];
  if (!["runtime", "operator"].includes(role))
    throw new Error("Unknown database workload.");
  return [
    role === "operator" ? "operator-db" : "runtime-db",
    "login-key",
    "wrap-key",
    "cache-url",
    "limiter-url",
    "ca",
    ...(role === "operator" ? ["limiter-admin-url"] : []),
  ];
}
export function runtimeEnvironment(c, role = "runtime") {
  const database = secretKeys(role)[0];
  const common = [
    env("DARKHORSE_DATABASE_POOL_SIZE", role === "runtime" ? "5" : "2"),
    env("PGSSLROOTCERT", "/run/secrets/ca"),
    env("DARKHORSE_DATABASE_URL_FILE", `/run/secrets/${database}`),
  ];
  if (role === "migrator") return common;
  return [
    ...common,
    env("DARKHORSE_PUBLIC_ORIGIN", c.origin),
    env("DARKHORSE_LOGIN_ENABLED", "true"),
    env("DARKHORSE_PROVIDER_ENABLED", role === "account" ? "false" : "true"),
    env("DARKHORSE_REDIS_CACHE_CONNECTIONS", role === "account" ? "1" : "2"),
    env("DARKHORSE_REDIS_LIMITER_CONNECTIONS", role === "runtime" ? "4" : "1"),
    ...[
      ["LOGIN_LIMIT_KEY", "login-key"],
      ...(role === "account" ? [] : [["SIGNING_WRAP_KEY", "wrap-key"]]),
      ["REDIS_CACHE_URL", "cache-url"],
      ["REDIS_LIMITER_URL", "limiter-url"],
      ["REDIS_CACHE_CA_PEM", "ca"],
      ["REDIS_LIMITER_CA_PEM", "ca"],
      ...(role === "operator"
        ? [["REDIS_LIMITER_ADMIN_URL", "limiter-admin-url"]]
        : []),
    ].map(([key, file]) =>
      env(`DARKHORSE_${key}_FILE`, `/run/secrets/${file}`),
    ),
  ];
}
export function pod(c, role = "runtime") {
  return {
    serviceAccountName: `darkhorse-${role}`,
    automountServiceAccountToken: false,
    enableServiceLinks: false,
    securityContext: {
      runAsNonRoot: true,
      runAsUser: 10001,
      runAsGroup: 10001,
      fsGroup: 10001,
      seccompProfile: { type: "RuntimeDefault" },
    },
    terminationGracePeriodSeconds: 30,
    containers: [
      {
        name: "api",
        image: c.image,
        imagePullPolicy: "IfNotPresent",
        args: ["serve"],
        securityContext: security(),
        env: runtimeEnvironment(c, role),
        resources: {
          requests: { cpu: "250m", memory: "256Mi" },
          limits: { cpu: role === "runtime" ? "2" : "1", memory: "512Mi" },
        },
        volumeMounts: [
          { name: "identity", mountPath: "/run/secrets", readOnly: true },
          { name: "temporary", mountPath: "/tmp" },
        ],
      },
    ],
    volumes: [
      {
        name: "identity",
        secret: {
          secretName: `darkhorse-${role === "account" ? "runtime" : role}-secrets`,
          items: secretKeys(role).map((key) => ({ key, path: key })),
          defaultMode: 0o440,
        },
      },
      { name: "temporary", emptyDir: { medium: "Memory", sizeLimit: "16Mi" } },
    ],
  };
}
export function servingPod(c) {
  const p = pod(c),
    api = p.containers[0];
  api.env.push(env("DARKHORSE_HTTP_HOST", "0.0.0.0"));
  api.ports = [{ name: "api", containerPort: 3001 }];
  api.startupProbe = {
    httpGet: { path: "/health/live", port: "api" },
    periodSeconds: 2,
    timeoutSeconds: 2,
    failureThreshold: 30,
  };
  api.livenessProbe = {
    httpGet: { path: "/health/live", port: "api" },
    periodSeconds: 10,
    timeoutSeconds: 2,
    failureThreshold: 3,
  };
  api.readinessProbe = {
    httpGet: { path: "/health/ready", port: "api" },
    periodSeconds: 5,
    timeoutSeconds: 2,
    failureThreshold: 1,
  };
  p.containers.push({
    name: "edge",
    image: c.edgeImage,
    imagePullPolicy: "IfNotPresent",
    securityContext: security(),
    env: [env("DARKHORSE_STACK_HOST", new URL(c.origin).hostname)],
    ports: [{ name: "https", containerPort: 8443 }],
    resources: {
      requests: { cpu: "50m", memory: "64Mi" },
      limits: { cpu: "250m", memory: "128Mi" },
    },
    startupProbe: {
      tcpSocket: { port: "https" },
      periodSeconds: 2,
      failureThreshold: 30,
    },
    livenessProbe: {
      tcpSocket: { port: "https" },
      periodSeconds: 10,
      failureThreshold: 3,
    },
    readinessProbe: {
      tcpSocket: { port: "https" },
      periodSeconds: 5,
      failureThreshold: 1,
    },
    volumeMounts: [
      { name: "edge-config", mountPath: "/etc/caddy", readOnly: true },
      { name: "edge-tls", mountPath: "/run/tls", readOnly: true },
      { name: "edge-data", mountPath: "/data" },
      { name: "edge-state", mountPath: "/config" },
    ],
  });
  p.volumes.push(
    { name: "edge-config", configMap: { name: "darkhorse-edge" } },
    {
      name: "edge-tls",
      secret: { secretName: "darkhorse-edge-tls", defaultMode: 0o440 },
    },
    { name: "edge-data", emptyDir: { medium: "Memory", sizeLimit: "8Mi" } },
    { name: "edge-state", emptyDir: { medium: "Memory", sizeLimit: "8Mi" } },
  );
  p.affinity = {
    podAntiAffinity: {
      preferredDuringSchedulingIgnoredDuringExecution: [
        {
          weight: 100,
          podAffinityTerm: {
            labelSelector: { matchLabels: labels("server") },
            topologyKey: "kubernetes.io/hostname",
          },
        },
      ],
    },
  };
  return p;
}
export const proxy = `{
 admin off
 auto_https off
 persist_config off
}
https://:8443 {
 tls /run/tls/tls.crt /run/tls/tls.key
 @canonical host {$DARKHORSE_STACK_HOST}
 handle @canonical {
  request_body {
   max_size 5MB
  }
  reverse_proxy 127.0.0.1:3001 {
   header_up Host {http.request.hostport}
   header_up -Forwarded
   header_up -X-Forwarded-For
   header_up -X-Forwarded-Host
   header_up -X-Forwarded-Proto
  }
 }
 handle {
  respond "Misdirected request" 421
 }
}
`;
