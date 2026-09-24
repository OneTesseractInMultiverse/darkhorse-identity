import test from "node:test";
import assert from "node:assert/strict";
import {
  accountOptions,
  composeAccount,
  kubernetesAccount,
  exitCode,
  interrupted,
  UsageError,
} from "../../lib/account-plan.mjs";
const id = "00000000-0000-0000-0000-000000000123";
const options = { ACCOUNT_OPERATION: "show", ACCOUNT_ID: id };
test("account launchers reject catalog selectors rather than running an inherited account mutation", () => {
  for (const key of [
    "CATALOG_TARGET",
    "CATALOG_OPERATION",
    "CATALOG_APPLICATION_ID",
    "CATALOG_SEARCH",
    "CATALOG_STATUS",
    "CATALOG_AFTER",
    "CATALOG_LIMIT",
    "CATALOG_CONFIRM",
    "CATALOG_REVISION",
  ])
    assert.throws(
      () => accountOptions({ ...options, [key]: "unexpected" }, false),
      UsageError,
    );
});
test("account launchers accept only bounded existing commands and protected stdin", () => {
  assert.deepEqual(accountOptions(options, false), [
    "--auth-stdin",
    "--output",
    "json",
    "operator",
    "account",
    "show",
    id,
  ]);
  const revoke = accountOptions(
    {
      ...options,
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "42",
      ACCOUNT_CONFIRM: "yes",
    },
    false,
  );
  assert.deepEqual(revoke, [
    "--auth-stdin",
    "--output",
    "json",
    "--yes",
    "operator",
    "account",
    "revoke-all",
    id,
    "42",
  ]);
  assert.ok(
    !accountOptions(
      { ...options, ACCOUNT_OPERATION: "deactivate", ACCOUNT_REVISION: "0" },
      false,
    ).includes("--yes"),
  );
  for (const change of [
    { ACCOUNT_OPERATION: "migrate" },
    { ACCOUNT_OPERATION: "show; echo bad" },
    { ACCOUNT_ID: "--password" },
    { ACCOUNT_ID: "00000000-0000-0000-0000-000000000000" },
    { ACCOUNT_ID: "a".repeat(4097) },
    { ACCOUNT_REVISION: "0" },
    { ACCOUNT_CONFIRM: "true" },
    { ACCOUNT_OPERATION: "deactivate" },
    { ACCOUNT_OPERATION: "reactivate", ACCOUNT_REVISION: "-1" },
    {
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "9223372036854775808",
    },
  ])
    assert.throws(
      () => accountOptions({ ...options, ...change }, false),
      UsageError,
    );
  assert.throws(() => accountOptions(options, true), /protected stdin/);
});
test("Compose account commands never select privileged services, a shell, or a TTY", () => {
  const args = accountOptions(options, false);
  assert.deepEqual(composeAccount("compose-exec", args), [
    "exec",
    "-T",
    "--user",
    "10001:10001",
    "api",
    "/usr/local/bin/darkhorse-server",
    ...args,
  ]);
  assert.deepEqual(
    composeAccount("compose-run", args, "darkhorse-account-1234567890abcdef"),
    [
      "run",
      "--rm",
      "--no-deps",
      "-T",
      "--name",
      "darkhorse-account-1234567890abcdef",
      "account",
      ...args,
    ],
  );
  for (const name of ["", "--privileged", "bad space", "../target"])
    assert.throws(() => composeAccount("compose-run", args, name));
  assert.throws(() => composeAccount("invalid", args));
});
test("Kubernetes execution requires explicit cluster, namespace, Pod and api container", () => {
  const values = {
    KUBE_ACCESS: "/private/access.yaml",
    KUBE_CONTEXT: "cluster-1",
    ACCOUNT_POD: "darkhorse-123-a",
  };
  const args = accountOptions(options, false);
  assert.deepEqual(kubernetesAccount("identity-app", values, args), [
    "--kubeconfig",
    values.KUBE_ACCESS,
    "--context",
    values.KUBE_CONTEXT,
    "--namespace",
    "identity-app",
    "--request-timeout=30s",
    "exec",
    "--pod-running-timeout=15s",
    "-i",
    "pod/darkhorse-123-a",
    "--container",
    "api",
    "--",
    "/usr/local/bin/darkhorse-server",
    ...args,
  ]);
  for (const change of [
    { KUBE_ACCESS: "" },
    { KUBE_ACCESS: "relative.yaml" },
    { KUBE_CONTEXT: "" },
    { KUBE_CONTEXT: "bad\ncontext" },
    { ACCOUNT_POD: "deployment/darkhorse" },
    { ACCOUNT_POD: "--privileged" },
    { ACCOUNT_POD: "a".repeat(254) },
  ])
    assert.throws(() =>
      kubernetesAccount("identity-app", { ...values, ...change }, args),
    );
});
test("exit status preserves explicit child failure and rejects ambiguous termination", () => {
  for (const code of [0, 1, 2, 3, 74, 125, 255])
    assert.equal(exitCode({ code, signal: null }), code);
  for (const [signal, code] of [
    ["SIGINT", 130],
    ["SIGTERM", 143],
    ["SIGHUP", 129],
  ])
    assert.deepEqual(interrupted(signal), { code, uncertain: true });
  for (const result of [
    { code: null, signal: "SIGKILL" },
    { code: -1 },
    { code: 256 },
    { code: NaN },
  ])
    assert.equal(exitCode(result), 1);
});

test("list launchers preserve literal filters, bound pages and reject stale single-target selectors", () => {
  const base = { ACCOUNT_OPERATION: "list" };
  assert.deepEqual(accountOptions(base, false), [
    "--auth-stdin",
    "--output",
    "json",
    "operator",
    "account",
    "list",
    "--limit",
    "25",
  ]);
  assert.deepEqual(
    accountOptions(
      {
        ...base,
        ACCOUNT_SEARCH: "literal $(text)",
        ACCOUNT_STATUS: "inactive",
        ACCOUNT_AFTER: id,
        ACCOUNT_LIMIT: "2",
      },
      false,
    ).slice(5),
    [
      "list",
      "--limit",
      "2",
      "--search=literal $(text)",
      "--status",
      "inactive",
      "--after",
      id,
    ],
  );
  assert.ok(
    accountOptions(
      {
        ...base,
        ACCOUNT_SEARCH: "--help",
        ACCOUNT_CONFIRM: "yes",
        ACCOUNT_LIMIT: "1",
      },
      false,
    ).includes("--search=--help"),
  );
  for (const change of [
    { ACCOUNT_ID: id },
    { ACCOUNT_REVISION: "0" },
    { ACCOUNT_LIMIT: "26" },
    { ACCOUNT_LIMIT: "0" },
    { ACCOUNT_LIMIT: "01" },
    { ACCOUNT_AFTER: "bad" },
    { ACCOUNT_AFTER: "00000000-0000-0000-0000-000000000000" },
    { ACCOUNT_CONFIRM: "invalid" },
    { ACCOUNT_STATUS: "all" },
    { ACCOUNT_SEARCH: "x\n" },
    { ACCOUNT_SEARCH: "x".repeat(101) },
    { ACCOUNT_SEARCH: " trim" },
  ])
    assert.throws(
      () => accountOptions({ ...base, ...change }, false),
      UsageError,
    );
  assert.throws(() =>
    accountOptions({ ...options, ACCOUNT_SEARCH: "filter" }, false),
  );
});
