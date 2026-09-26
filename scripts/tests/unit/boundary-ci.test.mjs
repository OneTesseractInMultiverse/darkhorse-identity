import test from "node:test";
import assert from "node:assert/strict";
import {
  boundarySuite,
  resourceLabels,
  resourceIds,
  boundaryResult,
} from "../../lib/boundary-ci.mjs";
const summary = (passed, ignored = 0, failed = 0, filtered = 0) =>
  `test result: ${failed ? "FAILED" : "ok"}. ${passed} passed; ${failed} failed; ${ignored} ignored; 0 measured; ${filtered} filtered out; finished in 1.00s\n`;
const worker =
  "test multiprocess_worker ... ignored, executed by the separate-process parent scenario with disposable infrastructure\n";
const parent =
  "test separate_processes_share_one_budget_without_shared_connection_pools ... ok\n";
const result = (stdout, code = 0) => ({
  code,
  stdout,
  interrupted: false,
  overflow: false,
});
test("only explicit boundary suites and owned Docker identifiers are accepted", () => {
  assert.equal(boundarySuite(["postgres"]), "postgres");
  assert.equal(boundarySuite(["redis"]), "redis");
  for (const input of [[], ["all"], ["redis", "--skip"], [";echo token"]])
    assert.throws(() => boundarySuite(input));
  assert.deepEqual(resourceLabels(undefined), []);
  assert.deepEqual(resourceLabels("a".repeat(32)), [
    "--label",
    `org.darkhorse.boundary-run=${"a".repeat(32)}`,
  ]);
  for (const value of ["", "a".repeat(31), "a".repeat(33), "-all", "abc\n"])
    assert.throws(() => resourceLabels(value));
  assert.deepEqual(resourceIds(`${"b".repeat(64)}\n`), ["b".repeat(64)]);
  assert.deepEqual(resourceIds(""), []);
  for (const value of [
    "some-container",
    "--all",
    "a".repeat(64) + " x",
    ("b".repeat(64) + "\n").repeat(101),
  ])
    assert.throws(() => resourceIds(value));
});
test("complete suites report actual counts and separately invoked worker handling", () => {
  const pg = boundaryResult("postgres", result(summary(251)));
  assert.equal(pg.status, "passed");
  assert.deepEqual(pg.suites, [
    { passed: 251, failed: 0, ignored: 0, measured: 0, filtered: 0 },
  ]);
  const redis = boundaryResult(
    "redis",
    result(summary(5) + worker + parent + summary(34, 1)),
  );
  assert.equal(redis.status, "passed");
  assert.equal(redis.worker, "executed by passing parent scenario");
});
test("failed, incomplete, filtered and accidentally skipped execution cannot qualify", () => {
  for (const output of [
    "",
    summary(0),
    summary(1, 1),
    summary(1, 0, 0, 1),
    summary(1, 0, 1),
    summary(1) + summary(1),
  ])
    assert.equal(boundaryResult("postgres", result(output)).status, "failed");
  for (const output of [
    summary(5),
    summary(5) + summary(34, 1),
    summary(5) + worker + summary(34, 1),
    summary(5) + worker + parent + summary(34, 2),
    summary(5) + parent + summary(34, 0),
  ])
    assert.equal(boundaryResult("redis", result(output)).status, "failed");
  for (const extra of [
    { code: 3 },
    { code: null },
    { interrupted: true },
    { overflow: true },
  ])
    assert.equal(
      boundaryResult("postgres", { ...result(summary(10)), ...extra }).status,
      "failed",
    );
});
test("reports disclose bounded test identifiers and counts rather than failure payloads", () => {
  const raw =
    "postgres://private:credential@host/db\ntest accounts::denied ... FAILED\nsecret-token\n" +
    summary(3, 0, 1);
  const report = boundaryResult("postgres", result(raw, 101));
  assert.deepEqual(report.failedTests, ["accounts::denied"]);
  assert.ok(!JSON.stringify(report).includes("credential"));
  assert.ok(!JSON.stringify(report).includes("secret-token"));
});

test("metadata records bounded version identifiers, never arbitrary command output", async () => {
  const { toolVersions } = await import("../../lib/boundary-ci.mjs");
  assert.deepEqual(
    toolVersions({
      rust: "rustc 1.97.1 (hash date)",
      docker: "29.6.2 / 29.6.2\n",
      openssl: "OpenSSL 3.0.13 date (Library: OpenSSL 3.0.13)",
    }),
    {
      rust: "1.97.1",
      dockerClient: "29.6.2",
      dockerServer: "29.6.2",
      openssl: "OpenSSL 3.0.13",
    },
  );
  assert.throws(() =>
    toolVersions({ rust: "secret", docker: "anything", openssl: "bad" }),
  );
});
