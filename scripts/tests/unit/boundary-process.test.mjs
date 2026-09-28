import test from "node:test";
import assert from "node:assert/strict";
import { boundaryProcess } from "../../lib/boundary-process.mjs";
import { boundaryResult } from "../../lib/boundary-ci.mjs";

const summary = (passed, ignored = 0, filtered = 0) =>
  `test result: ok. ${passed} passed; 0 failed; ${ignored} ignored; 0 measured; ${filtered} filtered out; finished in 1.00s\n`;

test("private boundary evidence combines test records from stdout and stderr", async () => {
  const outage =
    "test resource_introspection::redis_cache_outage_keeps_postgres_introspection_authoritative ... ok\n";
  const stderr =
    "test multiprocess_worker ... ignored, executed by the separate-process parent scenario with disposable infrastructure\n" +
    "test separate_processes_share_one_budget_without_shared_connection_pools ... ok\n" +
    outage;
  const stdout = summary(5) + summary(34, 1) + summary(1, 0, 289);
  const script = `process.stderr.write(${JSON.stringify(stderr)}); process.stdout.write(${JSON.stringify(stdout)});`;
  const result = await boundaryProcess(process.execPath, ["-e", script]);

  assert.equal(result.code, 0);
  assert.equal(result.overflow, false);
  assert.ok(result.stdout.includes(outage));
  assert.equal(boundaryResult("redis", result).status, "passed");
});
