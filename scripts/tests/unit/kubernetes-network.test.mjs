import test from "node:test";
import assert from "node:assert/strict";
import { backendPolicies, policies } from "../../lib/kubernetes-network.mjs";
import { application } from "../../lib/kubernetes-plan.mjs";

const c = {
  namespace: "identity-app",
  backendNamespace: "identity-state",
  ingressNamespace: "identity-ingress",
};
const labels = (role) => ({
  "app.kubernetes.io/name": "darkhorse",
  "app.kubernetes.io/component": role,
});
const matches = (selector, actual) =>
  Object.entries(selector.matchLabels).every(([key, v]) => actual[key] === v);
function admits(policy, namespace, source, port, protocol = "TCP") {
  return policy.spec.ingress.some(
    (rule) =>
      rule.ports.some((p) => p.port === port && p.protocol === protocol) &&
      rule.from.some(
        (peer) =>
          matches(peer.namespaceSelector, {
            "kubernetes.io/metadata.name": namespace,
          }) && matches(peer.podSelector, source),
      ),
  );
}

test("backend ingress admits only the dependency matrix in the selected application namespace", () => {
  const rules = backendPolicies(c);
  assert.equal(rules.length, 3);
  for (const [backend, port, allowed] of [
    ["postgres", 5432, ["server", "operator", "migrator", "account"]],
    ["cache", 6379, ["server", "operator"]],
    ["limiter", 6379, ["server", "operator", "account"]],
  ]) {
    const rule = rules.find((v) =>
      matches(v.spec.podSelector, labels(backend)),
    );
    assert.ok(rule);
    assert.equal(rule.metadata.namespace, c.backendNamespace);
    assert.deepEqual(rule.spec.policyTypes, ["Ingress"]);
    assert.equal(rule.spec.egress, undefined);
    for (const role of ["server", "operator", "migrator", "account", "unknown"])
      assert.equal(
        admits(rule, c.namespace, labels(role), port),
        allowed.includes(role),
      );
    for (const role of allowed) {
      assert.equal(admits(rule, c.backendNamespace, labels(role), port), false);
      assert.equal(admits(rule, c.ingressNamespace, labels(role), port), false);
      assert.equal(
        admits(
          rule,
          c.namespace,
          { "app.kubernetes.io/component": role },
          port,
        ),
        false,
      );
      assert.equal(admits(rule, c.namespace, labels(role), port + 1), false);
      assert.equal(admits(rule, c.namespace, labels(role), port, "UDP"), false);
    }
  }
});

test("backend ingress stays a separate opt-in manifest with no namespace adoption or cluster authority", () => {
  const rules = backendPolicies(c);
  assert.ok(rules.every((v) => v.kind === "NetworkPolicy"));
  assert.equal(new Set(rules.map((v) => v.metadata.name)).size, 3);
  for (const rule of rules) {
    assert.equal(rule.apiVersion, "networking.k8s.io/v1");
    assert.equal(
      rule.spec.podSelector.matchLabels["app.kubernetes.io/name"],
      "darkhorse",
    );
    assert.ok(rule.spec.podSelector.matchLabels["app.kubernetes.io/component"]);
  }
  const items = application({
    ...c,
    origin: "https://identity.example.com",
    image: "registry.example.com/app@sha256:" + "a".repeat(64),
    edgeImage: "registry.example.com/edge@sha256:" + "b".repeat(64),
  }).items;
  assert.ok(items.every((v) => v.metadata.namespace !== c.backendNamespace));
  assert.ok(policies(c).every((v) => v.metadata.namespace === c.namespace));
});
