import assert from "node:assert/strict";
import { isIP } from "node:net";
import { labels, security } from "./kubernetes-pods.mjs";

// The first application operation in each fresh Pod is the forbidden TCP probe.
// No readiness wait, DNS lookup or retry precedes it inside the container.
function startupProbe(c, name, namespace, role, target) {
  return {
    apiVersion: "v1",
    kind: "Pod",
    metadata: { name, namespace, labels: labels(role) },
    spec: {
      automountServiceAccountToken: false,
      enableServiceLinks: false,
      restartPolicy: "Never",
      activeDeadlineSeconds: 20,
      terminationGracePeriodSeconds: 1,
      containers: [
        {
          name: "probe",
          image: c.image,
          imagePullPolicy: "IfNotPresent",
          securityContext: security(),
          resources: {
            requests: { cpu: "25m", memory: "32Mi" },
            limits: { cpu: "100m", memory: "64Mi" },
          },
          command: [
            "bash",
            "-c",
            'timeout 2 bash -c \'exec 3<>"/dev/tcp/$1/$2"\' probe "$1" "$2"; result=$?; printf "%s\\n" "$result"; test "$result" -eq 124',
            "probe",
            target.ip,
            String(target.port),
          ],
        },
      ],
    },
  };
}

async function targets({ c, kube }) {
  const result = {};
  for (const [role, port] of [
    ["postgres", 5432],
    ["cache", 6379],
    ["limiter", 6379],
  ]) {
    const p = JSON.parse(
      (
        await kube([
          "-n",
          c.backendNamespace,
          "get",
          `pod/${role}`,
          "-o",
          "json",
        ])
      ).stdout,
    );
    assert.ok(
      isIP(p.status.podIP),
      "fixture backend requires a concrete Pod address",
    );
    result[role] = { ip: p.status.podIP, port };
  }
  return result;
}

async function reachable({ c, kube }, destinations) {
  for (const { ip, port } of Object.values(destinations)) {
    await kube([
      "-n",
      c.namespace,
      "exec",
      "operator",
      "--",
      "timeout",
      "3",
      "bash",
      "-c",
      'exec 3<>"/dev/tcp/$1/$2"',
      "probe",
      ip,
      String(port),
    ]);
  }
}

async function deniedStartup(fixture, namespace, role, target, index) {
  const { c, kube, apply } = fixture;
  const name = `startup-probe-${index}`;
  await apply(startupProbe(c, name, namespace, role, target));
  try {
    await kube([
      "-n",
      namespace,
      "wait",
      `pod/${name}`,
      "--for=jsonpath={.status.containerStatuses[0].state.terminated}",
      "--timeout=30s",
    ]);
    const p = JSON.parse(
      (await kube(["-n", namespace, "get", `pod/${name}`, "-o", "json"]))
        .stdout,
    );
    const log = (await kube(["-n", namespace, "logs", name])).stdout.trim();
    assert.equal(
      log,
      "124",
      `${namespace}/${role} must time out on its first forbidden connection to ${target.port}; observed ${log}`,
    );
    assert.equal(p.status.containerStatuses[0].state.terminated.exitCode, 0);
    return p.status.podIP;
  } finally {
    await kube([
      "-n",
      namespace,
      "delete",
      `pod/${name}`,
      "--wait=true",
      "--timeout=30s",
    ]);
  }
}

export async function startupIsolation(fixture) {
  const { c } = fixture;
  const destinations = await targets(fixture);
  await reachable(fixture, destinations);
  const scenarios = [
    ...Array.from({ length: 12 }, () => [c.namespace, "account", "cache"]),
    [c.namespace, "migrator", "cache"],
    [c.namespace, "migrator", "limiter"],
    // This namespace has no egress policy: these checks require backend ingress.
    ...["postgres", "cache", "limiter"].map((target) => [
      c.ingressNamespace,
      "server",
      target,
    ]),
    [c.ingressNamespace, "unknown", "cache"],
  ];
  const addresses = new Set();
  for (const [index, [namespace, role, target]] of scenarios.entries())
    addresses.add(
      await deniedStartup(
        fixture,
        namespace,
        role,
        destinations[target],
        index,
      ),
    );
  await reachable(fixture, destinations);
  console.log(
    `Backend ingress: ${scenarios.length} fresh-Pod first-connection denials, ${addresses.size} distinct source addresses; all three backend positive controls passed before and after. Backend policies were already active; CNI failure and address reuse are not qualified.`,
  );
}
