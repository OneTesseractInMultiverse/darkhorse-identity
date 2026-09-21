import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { clientFixture } from "./kubernetes-fixture.mjs";
export async function isolation({ c, kube, apply }, pods, ca) {
  for (const namespace of [c.ingressNamespace, c.backendNamespace]) {
    await apply({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: "probe-trust", namespace },
      data: { ca: ca.toString("base64") },
    });
    await apply(clientFixture(c, namespace));
    await kube([
      "-n",
      namespace,
      "wait",
      "--for=condition=Ready",
      "pod/probe",
      "--timeout=60s",
    ]);
  }
  const target = pods[0].status.podIP,
    host = new URL(c.origin).hostname,
    port = new URL(c.origin).port || "443";
  const args = [
    "curl",
    "--silent",
    "--show-error",
    "--max-time",
    "3",
    "--cacert",
    "/run/trust/ca",
    "--connect-to",
    `${host}:${port}:${target}:8443`,
    `${c.origin}/.well-known/openid-configuration`,
  ];
  const allowed = await kube([
    "-n",
    c.ingressNamespace,
    "exec",
    "probe",
    "--",
    ...args,
  ]);
  assert.equal(JSON.parse(allowed.stdout).issuer, c.origin);
  const denied = await kube(
    ["-n", c.backendNamespace, "exec", "probe", "--", ...args],
    { acceptFailure: true },
  );
  assert.notEqual(denied.code, 0, "unapproved namespace cannot reach HTTPS");
  const direct = await kube(
    [
      "-n",
      c.ingressNamespace,
      "exec",
      "probe",
      "--",
      "curl",
      "--silent",
      "--show-error",
      "--max-time",
      "3",
      `http://${target}:3001/health/live`,
    ],
    { acceptFailure: true },
  );
  assert.notEqual(direct.code, 0, "ingress cannot bypass TLS to API port");
  for (const p of pods) {
    const r = await kube([
      "-n",
      c.namespace,
      "exec",
      p.metadata.name,
      "-c",
      "api",
      "--",
      "sh",
      "-c",
      "test ! -e /var/run/secrets/kubernetes.io/serviceaccount/token && test ! -e /run/secrets/owner-db && test ! -e /run/secrets/limiter-admin-url",
    ]);
    assert.equal(r.code, 0);
    assert.notEqual(
      (
        await kube(
          [
            "-n",
            c.namespace,
            "exec",
            p.metadata.name,
            "-c",
            "api",
            "--",
            "darkhorse-server",
            "migrate",
          ],
          { acceptFailure: true },
        )
      ).code,
      0,
    );
  }
  console.log(
    "Actual network policies reject unapproved namespaces and direct API ingress; runtime secret/authority isolation passed.",
  );
}
async function databaseSignal({ c, kube }, signal) {
  assert.ok(["STOP", "CONT"].includes(signal));
  await kube(
    [
      "-n",
      c.backendNamespace,
      "exec",
      "postgres",
      "--",
      "sh",
      "-c",
      "kill -" +
        signal +
        ' "$(head -n 1 /data/db/postmaster.pid)"; for item in /proc/[0-9]*/comm; do read -r process < "$item" || continue; case "$process" in postgres*) pid=${item#/proc/}; pid=${pid%/comm}; kill -' +
        signal +
        ' "$pid";; esac; done',
    ],
    { signal: undefined },
  );
}
export async function availability(
  { c, kube, waitPods, signal },
  pods,
  requests,
  protocol,
) {
  const restarts = pods.map((p) =>
    p.status.containerStatuses.map((v) => v.restartCount),
  );
  await databaseSignal({ c, kube }, "STOP");
  try {
    const state = await kube([
      "-n",
      c.backendNamespace,
      "exec",
      "postgres",
      "--",
      "sh",
      "-c",
      'for item in /proc/[0-9]*/comm; do read -r process < "$item" || continue; case "$process" in postgres*) cat "${item%/comm}/status" | sed -n "/^Name:/p; /^State:/p";; esac; done',
    ]);
    const states = state.stdout
      .split("\n")
      .filter((line) => line.startsWith("State:"));
    assert.ok(
      states.length > 2 && states.every((line) => line.includes("T (stopped)")),
      "fixture must stop the postmaster and all database workers",
    );

    for (const request of requests) {
      assert.equal(
        (await request("/health/live")).status,
        200,
        "database outage keeps process live",
      );
      const health = await request("/health/ready");
      assert.equal(health.status, 503, "database outage fails readiness");
      assert.equal(
        (await protocol.introspect(request)).status,
        503,
        "database outage fails authoritative introspection",
      );
    }
    await delay(12000, undefined, { signal });
    const unavailable = JSON.parse(
      (
        await kube([
          "-n",
          c.namespace,
          "get",
          "pods",
          "-l",
          "app.kubernetes.io/component=server",
          "-o",
          "json",
        ])
      ).stdout,
    ).items;
    assert.ok(
      unavailable.every(
        (p) =>
          p.status.conditions.find((v) => v.type === "Ready").status ===
          "False",
      ),
    );
    assert.deepEqual(
      unavailable.map((p) =>
        p.status.containerStatuses.map((v) => v.restartCount),
      ),
      restarts,
      "database outage must not trigger liveness restarts",
    );
  } finally {
    await databaseSignal({ c, kube }, "CONT");
  }
  await waitPods(c.namespace, "app.kubernetes.io/component=server");
  await protocol.check(true);
  console.log(
    "Primary outage removes both replicas from readiness, rejects introspection, preserves liveness and recovers without restarts.",
  );
}
