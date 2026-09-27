import assert from "node:assert/strict";
import { test } from "node:test";
import {
  cleanupDeletePlanSql,
  cleanupSelectionPlanSql,
  cleanupTableStatisticsSql,
  summarizeCleanupQueryPlan,
} from "../../lib/benchmark-cleanup-plan.mjs";

test("cleanup diagnostics inspect bounded work without executing the delete plan", () => {
  assert.match(
    cleanupSelectionPlanSql,
    /EXPLAIN \(ANALYZE, BUFFERS, FORMAT JSON\)/,
  );
  assert.match(
    cleanupSelectionPlanSql,
    /ORDER BY expires_ms,digest[\s\S]*LIMIT 100[\s\S]*FOR UPDATE SKIP LOCKED/,
  );
  assert.match(cleanupDeletePlanSql, /^EXPLAIN \(FORMAT JSON\)/);
  assert.doesNotMatch(cleanupDeletePlanSql, /EXPLAIN \(ANALYZE/);
  assert.match(cleanupDeletePlanSql, /WITH expired AS MATERIALIZED/);
  assert.match(cleanupTableStatisticsSql, /deadTuplesEstimate/);
  assert.match(cleanupTableStatisticsSql, /last_autovacuum/);
});

test("query plan projection retains index and buffer evidence but excludes query text", () => {
  const report = summarizeCleanupQueryPlan([
    {
      "Planning Time": 1.25,
      "Execution Time": 4.5,
      "Query Text": "SELECT secret_fixture_material",
      Plan: {
        "Node Type": "Limit",
        "Plan Rows": 100,
        "Actual Rows": 100,
        "Actual Loops": 1,
        "Shared Hit Blocks": 4,
        "Shared Read Blocks": 1,
        Plans: [
          {
            "Node Type": "Index Scan",
            "Relation Name": "authorization_requests",
            "Index Name": "authorization_expiry",
            "Plan Rows": 100,
            "Actual Rows": 100,
            "Actual Loops": 1,
            Plans: [],
          },
        ],
      },
    },
  ]);
  assert.equal(report.root.children[0].index, "authorization_expiry");
  assert.equal(report.root.sharedBuffers.read, 1);
  assert.equal(
    JSON.stringify(report).includes("secret_fixture_material"),
    false,
  );
});

test("query plan projection rejects incomplete and overlarge plans", () => {
  assert.throws(
    () => summarizeCleanupQueryPlan([]),
    /Invalid lifecycle cleanup query plan/,
  );
  assert.throws(
    () =>
      summarizeCleanupQueryPlan([
        { Plan: { "Node Type": "Scan", Plans: [null] } },
      ]),
    /Invalid lifecycle cleanup query plan/,
  );
});
