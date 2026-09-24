import test from "node:test";
import assert from "node:assert/strict";
import {
  accountPod,
  createdIdentity,
  readyIdentity,
  removal,
} from "../../lib/kubernetes-account-plan.mjs";
import { application, budgets } from "../../lib/kubernetes-plan.mjs";
const c = {
  namespace: "identity-app",
  backendNamespace: "identity-state",
  ingressNamespace: "identity-ingress",
  origin: "https://identity.example.com",
  image: "registry.example.com/darkhorse@sha256:" + "a".repeat(64),
  edgeImage: "registry.example.com/edge@sha256:" + "b".repeat(64),
};
const name = "darkhorse-account-0123456789abcdef";
const uid = "00000000-0000-0000-0000-000000000123";
const snapshot = () => ({
  ...accountPod(c, name),
  metadata: { ...accountPod(c, name).metadata, uid },
  status: {
    phase: "Running",
    containerStatuses: [
      { name: "api", ready: true, restartCount: 0, state: { running: {} } },
    ],
  },
});
test("one-shot account Pod has bounded lifetime, no replacement controller, no HTTP or privileged secrets", () => {
  const p = accountPod(c, name),
    s = p.spec,
    api = s.containers[0];
  assert.equal(p.kind, "Pod");
  assert.equal(p.metadata.ownerReferences, undefined);
  assert.equal(s.restartPolicy, "Never");
  assert.equal(s.activeDeadlineSeconds, 180);
  assert.equal(s.automountServiceAccountToken, false);
  assert.equal(s.serviceAccountName, "darkhorse-account");
  assert.equal(s.containers.length, 1);
  assert.deepEqual(api.command, ["/bin/sleep"]);
  assert.deepEqual(api.args, ["180"]);
  assert.equal(api.image, c.image);
  assert.equal(api.ports, undefined);
  assert.equal(api.securityContext.readOnlyRootFilesystem, true);
  assert.equal(api.securityContext.runAsUser, 10001);
  assert.equal(api.securityContext.allowPrivilegeEscalation, false);
  assert.deepEqual(api.securityContext.capabilities.drop, ["ALL"]);
  const secret = s.volumes.find((v) => v.name === "identity").secret;
  assert.equal(secret.secretName, "darkhorse-runtime-secrets");
  assert.deepEqual(secret.items.map((v) => v.key).sort(), [
    "ca",
    "cache-url",
    "limiter-url",
    "login-key",
    "runtime-db",
  ]);
  assert.ok(!JSON.stringify(s).includes("wrap-key"));
  const env = Object.fromEntries(api.env.map((v) => [v.name, v.value]));
  assert.equal(env.DARKHORSE_PROVIDER_ENABLED, "false");
  assert.equal(env.DARKHORSE_DATABASE_POOL_SIZE, "2");
  assert.equal(env.DARKHORSE_REDIS_LIMITER_CONNECTIONS, "1");
  assert.ok(
    Number(env.DARKHORSE_DATABASE_POOL_SIZE) * budgets().pods <=
      budgets().databaseConnections,
  );
  for (const bad of ["", "other", "../x", name + "x"])
    assert.throws(() => accountPod(c, bad));
});
test("account workload can reach only DNS, primary database and limiter", () => {
  const items = application(c).items;
  assert.ok(
    items.some(
      (v) =>
        v.kind === "ServiceAccount" && v.metadata.name === "darkhorse-account",
    ),
  );
  const p = items.find(
    (v) =>
      v.kind === "NetworkPolicy" && v.metadata.name === "darkhorse-account",
  ).spec;
  assert.deepEqual(p.ingress, []);
  assert.equal(p.egress.length, 3);
  assert.deepEqual(
    p.egress
      .slice(1)
      .map(
        (v) => v.to[0].podSelector.matchLabels["app.kubernetes.io/component"],
      ),
    ["postgres", "limiter"],
  );
  assert.ok(
    p.egress
      .slice(1)
      .every(
        (v) =>
          v.to[0].namespaceSelector.matchLabels[
            "kubernetes.io/metadata.name"
          ] === c.backendNamespace,
      ),
  );
});
test("execution rejects replaced, terminating, restarted or mismatched Pods", () => {
  const expected = accountPod(c, name);
  assert.equal(createdIdentity(snapshot(), expected), uid);
  assert.equal(readyIdentity(snapshot(), expected, uid), uid);
  for (const change of [
    (p) => {
      p.metadata.uid = "bad";
    },
    (p) => {
      p.metadata.name = "other";
    },
    (p) => {
      p.metadata.namespace = "elsewhere";
    },
    (p) => {
      p.metadata.deletionTimestamp = "now";
    },
    (p) => {
      p.spec.containers[0].image = "wrong:latest";
    },
    (p) => {
      p.status.phase = "Pending";
    },
    (p) => {
      p.status.containerStatuses[0].restartCount = 1;
    },
    (p) => {
      p.status.containerStatuses[0].ready = false;
    },
    (p) => {
      p.metadata.uid = "00000000-0000-0000-0000-000000000456";
    },
  ]) {
    const p = snapshot();
    change(p);
    assert.throws(() => readyIdentity(p, expected, uid));
  }
  assert.throws(() => createdIdentity({}, expected));
  const deletion = removal(expected, uid);
  assert.equal(deletion.path, `/api/v1/namespaces/${c.namespace}/pods/${name}`);
  assert.deepEqual(deletion.body.preconditions, { uid });
  assert.ok(deletion.body.gracePeriodSeconds > 0);
  assert.throws(() => removal(expected, ""));
});
