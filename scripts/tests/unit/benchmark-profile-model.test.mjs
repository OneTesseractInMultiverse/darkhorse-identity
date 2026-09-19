import assert from "node:assert/strict";
import { test } from "node:test";
import {
  parseProfile,
  STAGES,
  UPPER_US,
} from "../../lib/benchmark-profile-model.mjs";

function fixture() {
  return {
    schema: 1,
    upper_us: [...UPPER_US],
    stages: Object.fromEntries(
      STAGES.map((name) => [
        name,
        {
          ok: 1,
          error: 1,
          cancelled: 1,
          sum_us: 10000007,
          max_us: 10000001,
          buckets: [1, 1, ...Array(19).fill(0), 1],
        },
      ]),
    ),
  };
}
test("profile reports retain outcomes and bounded quantiles, with an explicit overflow", () => {
  const report = parseProfile(JSON.stringify(fixture()));
  assert.equal(report.stages.total.count, 3);
  assert.equal(report.stages.total.mean_us, 10000007 / 3);
  assert.deepEqual(report.stages.total.percentile_upper_us, {
    p50: 5,
    p95: null,
    p99: null,
  });
  const empty = fixture();
  for (const stage of Object.values(empty.stages))
    Object.assign(stage, {
      ok: 0,
      error: 0,
      cancelled: 0,
      sum_us: 0,
      max_us: 0,
      buckets: Array(22).fill(0),
    });
  assert.equal(parseProfile(JSON.stringify(empty)).stages.total.mean_us, null);
});
test("malformed, inconsistent or unexpected profiler data fails without echoing input", () => {
  for (const edit of [
    (r) => {
      r.schema = 2;
    },
    (r) => {
      r.secret = "SENSITIVE";
    },
    (r) => {
      r.upper_us[0] = 2;
    },
    (r) => {
      delete r.stages.total;
    },
    (r) => {
      r.stages.total.ok = -1;
    },
    (r) => {
      r.stages.total.sum_us = Infinity;
    },
    (r) => {
      r.stages.total.buckets[0] = 0;
    },
    (r) => {
      r.stages.total.extra = "SENSITIVE";
    },
    (r) => {
      r.stages.total.buckets.pop();
    },
    (r) => {
      r.stages.total.max_us = Number.MAX_SAFE_INTEGER + 1;
    },
  ]) {
    const report = fixture();
    edit(report);
    assert.throws(
      () => parseProfile(JSON.stringify(report)),
      /^Error: Invalid benchmark profile frame\.$/,
    );
  }
  for (const text of ["SENSITIVE", "null", "[]", "{}", "x".repeat(16385)])
    assert.throws(
      () => parseProfile(text),
      /^Error: Invalid benchmark profile frame\.$/,
    );
});
