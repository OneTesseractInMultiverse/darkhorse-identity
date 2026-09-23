import { isolation, availability } from "./lib/kubernetes-isolation-test.mjs";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { run } from "./lib/command.mjs";
import { setupStack, stackDirectory } from "./lib/deployment-state.mjs";
import { configuration, operatorJob } from "./lib/kubernetes-plan.mjs";
import { pod, labels } from "./lib/kubernetes-pods.mjs";
import { backendFixture } from "./lib/kubernetes-fixture.mjs";
import {
  nodeImage,
  loadImages,
  forward,
  localPort,
} from "./lib/kubernetes-cluster.mjs";
import {
  accountCommand,
  accountResult,
} from "./lib/container-account-test.mjs";
import { httpsCall } from "./lib/deployment-client.mjs";
import {
  replicaProtocol,
  sharedBudgets,
} from "./lib/replica-protocol-test.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
process.umask(0o077);
const name = `kube-${randomBytes(5).toString("hex")}`,
  directory = resolve(".local/kubernetes", name),
  access = join(directory, "access.yaml"),
  context = `kind-${name}`,
  kind = process.env.KIND ?? "kind";
const abort = new AbortController(),
  children = [];
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
const command = (tool, args, options = {}) =>
  run(tool, args, { signal: abort.signal, capture: true, ...options });
let created = false,
  stack,
  c;
const kube = (args, options = {}) =>
  command(
    "kubectl",
    [
      "--kubeconfig",
      access,
      "--context",
      context,
      "--request-timeout=30s",
      ...args,
    ],
    options,
  );
const apply = (value) =>
  kube(["apply", "-f", "-"], { input: JSON.stringify(value) });
const sql = (text) =>
  kube(
    [
      "-n",
      c.backendNamespace,
      "exec",
      "-i",
      "postgres",
      "--",
      "psql",
      "-U",
      "postgres",
      "-d",
      "darkhorse",
      "-At",
      "-v",
      "ON_ERROR_STOP=1",
    ],
    { input: text },
  );
const operator = (args, options = {}) =>
  kube(
    [
      "-n",
      c.namespace,
      "exec",
      "-i",
      "operator",
      "--",
      "darkhorse-server",
      "--yes",
      ...args,
    ],
    options,
  );
async function waitPods(namespace, selector) {
  await kube([
    "-n",
    namespace,
    "wait",
    "--for=condition=Ready",
    "pod",
    "--selector",
    selector,
    "--timeout=180s",
  ]);
}
async function publicCommand(mode) {
  return command(process.execPath, [
    "scripts/kubernetes.mjs",
    mode,
    join(directory, "configuration.json"),
    access,
    context,
  ]);
}
async function createCluster() {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  assert.match((await command(kind, ["version"])).stdout, /v0\.33\.0/);
  created = true;
  await command(kind, [
    "create",
    "cluster",
    "--name",
    name,
    "--image",
    nodeImage,
    "--kubeconfig",
    access,
    "--wait",
    "120s",
  ]);
  console.log(
    "Owned Kubernetes 1.36.4 cluster created with a private kubeconfig.",
  );
  const images = [
    process.env.DARKHORSE_TEST_IMAGE ?? "darkhorse:local",
    "darkhorse-edge:local",
  ];
  const [image, edgeImage] = await loadImages(name, kind, command, images);
  const origin = `https://identity.localhost:${await localPort()}`;
  stack = await setupStack(name, origin, images[0], command);
  c = configuration({
    namespace: "identity-app",
    origin,
    image,
    edgeImage,
    backendNamespace: "identity-state",
    ingressNamespace: "identity-ingress",
  });
}
async function prepareNamespace() {
  await writeFile(join(directory, "configuration.json"), JSON.stringify(c));
  await publicCommand("prepare");
  for (const args of [[], [access], [access, ""]]) {
    const result = await command(
      process.execPath,
      [
        "scripts/kubernetes.mjs",
        "status",
        join(directory, "configuration.json"),
        ...args,
      ],
      { acceptFailure: true },
    );
    assert.notEqual(result.code, 0, "never select ambient cluster context");
  }
  await apply(await backendFixture(c, stack.directory));
  await waitPods(c.backendNamespace, "app.kubernetes.io/name=darkhorse");
  console.log(
    "Isolated TLS PostgreSQL/Redis and restricted application namespace prepared.",
  );
  await publicCommand("validate");
  const rejected = join(directory, "rejected.json");
  await writeFile(
    rejected,
    JSON.stringify({
      ...c,
      namespace: c.backendNamespace,
      backendNamespace: c.namespace,
    }),
  );
  const adoption = await command(
    process.execPath,
    ["scripts/kubernetes.mjs", "prepare", rejected, access, context],
    { acceptFailure: true },
  );
  assert.notEqual(adoption.code, 0, "refuse to adopt an unmanaged namespace");
}
async function prepareOperator() {
  await apply(operatorJob(c, "migrate", "migration-1"));
  await kube([
    "-n",
    c.namespace,
    "wait",
    "--for=condition=complete",
    "job/migration-1",
    "--timeout=120s",
  ]);
  await sql(await readFile("deploy/grant-runtime.sql", "utf8"));
  const spec = pod(c, "operator");
  spec.containers[0].command = ["/bin/sleep"];
  spec.containers[0].args = ["1800"];
  await apply({
    apiVersion: "v1",
    kind: "Pod",
    metadata: {
      name: "operator",
      namespace: c.namespace,
      labels: labels("operator"),
    },
    spec,
  });
  await kube([
    "-n",
    c.namespace,
    "wait",
    "--for=condition=Ready",
    "pod/operator",
    "--timeout=60s",
  ]);
}
async function initializeIdentity() {
  assert.notEqual(
    (await operator(["migrate"], { acceptFailure: true })).code,
    0,
    "operator credentials cannot migrate even an up-to-date schema",
  );
  await kube([
    "-n",
    c.namespace,
    "exec",
    "operator",
    "--",
    "sh",
    "-c",
    "test -e /run/secrets/operator-db && test ! -e /run/secrets/owner-db",
  ]);

  const password = randomBytes(24).toString("base64url");
  const boot = await operator(["bootstrap", "--stdin"], {
    input: JSON.stringify({
      email: "replicas@example.com",
      first_name: "Replica",
      last_name: "Fixture",
      password,
    }),
  });
  const principal = boot.stdout.match(/[a-f0-9-]{36}/)[0];
  const key = JSON.parse((await operator(["signing-generate", "0"])).stdout);
  assert.notEqual(
    (
      await operator(["signing-activate", key.kid, "1"], {
        acceptFailure: true,
      })
    ).code,
    0,
    "publication delay is mandatory",
  );
  await sql(
    "ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-60001; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;",
  );
  await operator(["signing-activate", key.kid, "1"]);
  return { password, principal };
}
async function prepare() {
  await createCluster();
  await prepareNamespace();
  await prepareOperator();
  const user = await initializeIdentity();
  await recovery();
  await publicCommand("apply");
  await publicCommand("status");
  await kube([
    "-n",
    c.namespace,
    "rollout",
    "status",
    "deployment/darkhorse",
    "--timeout=180s",
  ]);
  return user;
}
async function recovery() {
  const state = JSON.parse((await operator(["limiter-fence"])).stdout);
  assert.ok(state.not_before_ms - state.database_ms >= 903000);
  assert.notEqual(
    (await operator(["limiter-activate"], { acceptFailure: true })).code,
    0,
  );
  await sql(
    "ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;",
  );
  const activated = JSON.parse(
    (await operator(["--output", "json", "operator", "limiter", "activate"]))
      .stdout,
  );
  const record = JSON.parse(
    (
      await operator([
        "operator",
        "limiter",
        "inspect",
        activated.data.operation_id,
      ])
    ).stdout,
  );
  assert.equal(record.recorded_outcome, "activated");
  assert.equal(record.database_role, "darkhorse_operator");
  assert.equal(record.same_generation, true);
}
async function servingPods() {
  return JSON.parse(
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
}
async function clients(pods, ca) {
  const ports = await Promise.all(
    pods.map((p) =>
      forward(access, context, c.namespace, p.metadata.name, children),
    ),
  );
  return ports.map(
    (connectPort) =>
      (path, options = {}) =>
        httpsCall(c.origin, ca, path, { ...options, connectPort }),
  );
}
async function rolling(requests, ca, protocol) {
  const previous = (await servingPods()).map((p) => p.metadata.uid);
  await kube(["-n", c.namespace, "rollout", "restart", "deployment/darkhorse"]);
  await kube([
    "-n",
    c.namespace,
    "rollout",
    "status",
    "deployment/darkhorse",
    "--timeout=180s",
  ]);
  const pods = (await servingPods()).filter(
    (p) => !p.metadata.deletionTimestamp,
  );
  assert.equal(pods.length, 2);
  assert.ok(pods.every((p) => !previous.includes(p.metadata.uid)));
  requests.splice(0, requests.length, ...(await clients(pods, ca)));
  await protocol.check(true);
  assert.equal((await protocol.browser("/api/auth/session")).status, 200);
  console.log(
    "Same-version rolling replacement preserves shared sessions and opaque credentials.",
  );
}
async function limiterContinuity(requests, protocol, account) {
  const snapshot = JSON.parse(
    (await kube(["-n", c.backendNamespace, "get", "pod/limiter", "-o", "json"]))
      .stdout,
  );
  const count = snapshot.status.containerStatuses[0].restartCount;
  await kube(
    [
      "-n",
      c.backendNamespace,
      "exec",
      "limiter",
      "--",
      "sh",
      "-c",
      "kill -TERM 1",
    ],
    { acceptFailure: true },
  );
  let restarted = false;
  for (let n = 0; n < 60; n++) {
    const current = JSON.parse(
      (
        await kube([
          "-n",
          c.backendNamespace,
          "get",
          "pod/limiter",
          "-o",
          "json",
        ])
      ).stdout,
    );
    if (
      current.status.containerStatuses[0].restartCount > count &&
      current.status.containerStatuses[0].ready
    ) {
      restarted = true;
      break;
    }
    await delay(1000, undefined, { signal: abort.signal });
  }
  assert.ok(restarted, "owned limiter restarted");
  const login = (request) =>
    request("/api/auth/login", {
      method: "POST",
      headers: {
        origin: c.origin,
        "x-darkhorse-csrf": "1",
        "content-type": "application/json",
      },
      body: JSON.stringify({
        email: "continuity@example.com",
        password: "not a real password for an account",
      }),
    });
  for (const request of requests)
    assert.equal((await login(request)).status, 503);
  await protocol.check(true);
  accountResult(await account(), 1, /Account operation unavailable/);
  await recovery();
  for (const request of requests)
    assert.equal((await login(request)).status, 401);
  console.log(
    "Limiter restart conservatively rejects sign-in on both replicas until explicit recovery; token checks retain authority.",
  );
}
async function main() {
  const user = await prepare();
  const list = await servingPods();
  assert.equal(list.length, 2);
  const ca = await readFile(join(stack.directory, "secrets/ca.pem"));
  const requests = await clients(list, ca);
  for (const request of requests) {
    assert.equal((await request("/health/live")).status, 200);
    assert.equal((await request("/health/ready")).status, 200);
    assert.equal(
      JSON.parse((await request("/.well-known/openid-configuration")).text)
        .issuer,
      c.origin,
    );
  }
  const protocol = await replicaProtocol({
    origin: c.origin,
    requests,
    ...user,
  });
  const checks = { c, kube, apply, waitPods, signal: abort.signal };
  await isolation(checks, list, ca);
  await availability(checks, list, requests, protocol);
  await rolling(requests, ca, protocol);
  const currentPod = (await servingPods()).find(
    (p) => !p.metadata.deletionTimestamp,
  ).metadata.name;
  const settings = {
    KUBE_CONFIG: join(directory, "configuration.json"),
    KUBE_ACCESS: access,
    KUBE_CONTEXT: context,
    ACCOUNT_POD: currentPod,
    ACCOUNT_ID: user.principal,
  };
  const auth = {
    email: "replicas@example.com",
    password: user.password,
    reason: "Verify replica revocation",
  };
  const account = (extra = {}, input = auth) =>
    accountCommand(command, "kube-exec", { ...settings, ...extra }, input);
  await limiterContinuity(requests, protocol, account);
  await sharedBudgets(c.origin, requests);
  await accountChecks(account, settings, auth, user);
  await protocol.check(false);
  console.log(
    "Cross-replica shared limiting and strict post-commit account revocation passed.",
  );
}
async function accountChecks(account, settings, auth, user) {
  assert.equal(accountResult(await account(), 0).id, user.principal);
  accountResult(
    await account({}, { ...auth, password: "source-defined-wrong-password" }),
    1,
    /Administrator authentication or authority denied/,
  );
  accountResult(
    await account({ ACCOUNT_OPERATION: "revoke-all", ACCOUNT_REVISION: "0" }),
    3,
    /confirmation_required/,
  );
  accountResult(
    await account({
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "99",
      ACCOUNT_CONFIRM: "yes",
    }),
    1,
    /principal changed/,
  );
  const revoked = accountResult(
    await account({
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "0",
      ACCOUNT_CONFIRM: "yes",
    }),
    0,
  );
  assert.equal(revoked.revision, 1);
  assert.equal(
    (
      await sql(
        "SELECT bool_and(database_role='darkhorse_runtime') FROM operator_account_audit;",
      )
    ).stdout.trim(),
    "t",
  );
  const made = await command(
    "make",
    [
      "--no-print-directory",
      "kube-account-exec",
      ...Object.entries(settings).map(([key, value]) => `${key}=${value}`),
      "ACCOUNT_OPERATION=show",
      "ACCOUNT_REVISION=",
      "ACCOUNT_CONFIRM=no",
    ],
    { input: JSON.stringify(auth) },
  );
  assert.equal(accountResult(made, 0).revision, 1);
  assert.ok(
    !made.stdout.includes(auth.password) &&
      !made.stderr.includes(auth.password),
  );
}
try {
  await main();
} catch (error) {
  console.error(error.stack);
  if (created) {
    const r = await kube(["get", "pods", "--all-namespaces", "-o", "wide"], {
      acceptFailure: true,
      signal: undefined,
    });
    console.error(r.stdout);
  }
  process.exitCode = 1;
} finally {
  for (const child of children) child.kill("SIGINT");
  if (created)
    await command(
      kind,
      ["delete", "cluster", "--name", name, "--kubeconfig", access],
      { signal: undefined, acceptFailure: true },
    );
  if (stack) await rm(stackDirectory(name), { recursive: true, force: true });
  await rm(directory, { recursive: true, force: true });
}
