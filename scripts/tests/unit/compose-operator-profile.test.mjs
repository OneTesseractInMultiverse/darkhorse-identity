import assert from "node:assert/strict";
import { test } from "node:test";
import {
  composeOperatorDatabaseProfile,
  parseComposeOperatorProfileLine,
} from "../../lib/compose-operator-profile.mjs";

const upperUs = [
  1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000, 25000, 50000,
  100000, 250000, 500000, 1000000, 2500000, 5000000, 10000000,
];

function histogram() {
  const buckets = Array(22).fill(0);
  buckets[5] = 2;
  buckets[7] = 1;
  return {
    ok: 2,
    error: 1,
    cancelled: 0,
    sum_us: 180,
    max_us: 100,
    buckets,
  };
}

function report() {
  return {
    schema: 2,
    upper_us: upperUs,
    stages: Object.fromEntries(
      [
        "service_slot",
        "service_handler",
        "admission_global",
        "admission_global_queue",
        "admission_global_counter",
        "admission_authenticated",
        "admission_caller_authentication",
        "admission_caller_counter",
        "limiter_slot",
        "limiter_operation",
        "total",
        "pool_acquire",
        "begin",
        "fence",
        "authenticate",
        "inspect",
        "policy_load",
        "decision",
        "commit",
        "client_total",
        "client_pool_acquire",
        "client_begin",
        "client_fence",
        "client_authenticate",
        "client_inspect",
        "client_commit",
      ].map((stage) => [stage, histogram()]),
    ),
  };
}

function records() {
  return ["control-before", "account-detail-overlap", "control-after"].map(
    (phase) => ({ phase, report: report() }),
  );
}

test("Compose profiler preserves only fixed bounded stage summaries", () => {
  const input = records();
  input[0].report.privateIdentity = "private@example.com";
  input[0].report.stages.total.query = "sensitive SQL";
  const result = composeOperatorDatabaseProfile(input);
  assert.deepEqual(result.bucketUpperUs, upperUs);
  assert.equal(result.phases.length, 3);
  assert.deepEqual(result.phases[0].stages.total, {
    ok: 2,
    error: 1,
    cancelled: 0,
    samples: 3,
    sumUs: 180,
    maxUs: 100,
    buckets: histogram().buckets,
  });
  assert.doesNotMatch(JSON.stringify(result), /private@example|sensitive SQL/);
});

test("Compose profiler parses a single report without depending on Docker logs", () => {
  const line = `api-1 | DARKHORSE_PROFILE ${JSON.stringify(report())}`;
  assert.deepEqual(parseComposeOperatorProfileLine(line, "control-before"), {
    phase: "control-before",
    report: report(),
  });
  assert.throws(
    () =>
      parseComposeOperatorProfileLine("unrelated log line", "control-before"),
    /profile line/,
  );
  assert.throws(
    () =>
      parseComposeOperatorProfileLine(
        "DARKHORSE_PROFILE {bad json}",
        "control-before",
      ),
    /profile payload/,
  );
});

test("Compose profiler rejects missing, duplicate, or unknown phase reports", () => {
  const input = records();
  assert.throws(
    () => composeOperatorDatabaseProfile(input.slice(0, 2)),
    /phases/,
  );
  const duplicate = records();
  duplicate[2].phase = duplicate[0].phase;
  assert.throws(() => composeOperatorDatabaseProfile(duplicate), /report/);
  const unknown = records();
  unknown[0].phase = "arbitrary";
  assert.throws(() => composeOperatorDatabaseProfile(unknown), /report/);
});

test("Compose profiler rejects altered stage sets and invalid histograms", () => {
  const missingStage = records();
  delete missingStage[0].report.stages.commit;
  assert.throws(
    () => composeOperatorDatabaseProfile(missingStage),
    /profile report/,
  );
  const malformed = records();
  malformed[1].report.stages.pool_acquire.error = -1;
  assert.throws(() => composeOperatorDatabaseProfile(malformed), /histogram/);
});
