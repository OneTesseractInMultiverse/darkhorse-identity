import assert from "node:assert/strict";
import { test } from "node:test";
import { verifyPhaseProfile } from "../../lib/benchmark-profile-consistency.mjs";
function fixture() {
  return {
    summary: {
      attempts: 10,
      outcomes: { authorized: 2, denied: 3, healthy: 1 },
    },
    profile: {
      rust: {
        stages: {
          total: { count: 5 },
          pool_acquire: { count: 5 },
          decision: { count: 2 },
        },
      },
      sql: { groups: [{ calls: 19 }] },
    },
  };
}
test("completed resource checks must be represented even when the phase includes overload or health requests", () => {
  const { profile, summary } = fixture();
  verifyPhaseProfile(profile, summary);
  for (const edit of [
    (p) => {
      p.rust.stages.total.count = 0;
    },
    (p) => {
      p.rust.stages.total.count = 10;
    },
    (p) => {
      p.rust.stages.pool_acquire.count = 4;
    },
    (p) => {
      p.rust.stages.decision.count = 1;
    },
    (p) => {
      p.sql.groups = [];
    },
  ]) {
    const { profile, summary } = fixture();
    edit(profile);
    assert.throws(
      () => verifyPhaseProfile(profile, summary),
      /Incomplete benchmark profiling/,
    );
  }
});
