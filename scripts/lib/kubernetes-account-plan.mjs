import { configuration } from "./kubernetes-plan.mjs";
import { labels, pod } from "./kubernetes-pods.mjs";
const uidPattern = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
export function accountPod(input, name) {
  const c = configuration(input);
  if (!/^darkhorse-account-[a-f0-9]{16}$/.test(name))
    throw new Error("Invalid one-shot account Pod name.");
  const spec = pod(c, "account");
  spec.restartPolicy = "Never";
  spec.activeDeadlineSeconds = 180;
  spec.terminationGracePeriodSeconds = 5;
  spec.containers[0].command = ["/bin/sleep"];
  spec.containers[0].args = ["180"];
  return {
    apiVersion: "v1",
    kind: "Pod",
    metadata: { name, namespace: c.namespace, labels: labels("account") },
    spec,
  };
}
export function createdIdentity(actual, expected) {
  const m = actual?.metadata;
  if (
    !uidPattern.test(m?.uid ?? "") ||
    m.name !== expected.metadata.name ||
    m.namespace !== expected.metadata.namespace
  )
    throw new Error("Account Pod creation identity is unavailable.");
  return m.uid;
}
export function readyIdentity(actual, expected, uid) {
  const identity = createdIdentity(actual, expected);
  const states = actual.status?.containerStatuses;
  if (
    identity !== uid ||
    actual.metadata.deletionTimestamp ||
    actual.spec?.containers?.length !== 1 ||
    actual.spec.containers[0].image !== expected.spec.containers[0].image ||
    actual.status?.phase !== "Running" ||
    states?.length !== 1 ||
    states[0].name !== "api" ||
    !states[0].ready ||
    states[0].restartCount !== 0 ||
    !states[0].state?.running
  )
    throw new Error("Account Pod identity or running state changed.");
  return identity;
}
export function removal(expected, uid) {
  if (!uidPattern.test(uid))
    throw new Error("A known Pod UID is required for cleanup.");
  const { name, namespace } = expected.metadata;
  return {
    path: `/api/v1/namespaces/${namespace}/pods/${name}`,
    body: {
      apiVersion: "v1",
      kind: "DeleteOptions",
      preconditions: { uid },
      gracePeriodSeconds: 5,
    },
  };
}
