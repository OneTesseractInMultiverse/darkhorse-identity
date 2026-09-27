import assert from "node:assert/strict";
import { test } from "node:test";
import { lifecycleFixtureStatements } from "../../lib/benchmark-cleanup-fixture.mjs";

test("lifecycle benchmark fixtures have bounded, separated synthetic populations", () => {
  const sql = lifecycleFixtureStatements(
    "fe0403e5-3210-4321-8765-123456789abc",
    { expiredRows: 20_000, liveRows: 20_000 },
  );
  assert.match(sql.seed, /generate_series\(1,20000\) AS expired/);
  assert.match(sql.seed, /generate_series\(1,20000\) AS live/);
  assert.match(
    sql.seed,
    /clock\.ms-600000 AS created_ms,clock\.ms-300000 AS expires_ms/,
  );
  assert.match(sql.seed, /created_ms,expires_ms/);
  assert.match(
    sql.seed,
    /FROM oauth_clients WHERE application_id='fe0403e5-3210-4321-8765-123456789abc'/,
  );
  assert.match(
    sql.counts,
    /substring\(request\.digest FROM 1 FOR 8\)=decode\('d4c1dead00000000','hex'\)/,
  );
  assert.match(sql.counts, /FILTER \(WHERE request\.expires_ms<=clock\.ms\)/);
  assert.match(sql.counts, /FILTER \(WHERE request\.expires_ms>clock\.ms\)/);
  assert.doesNotMatch(sql.seed, /ON CONFLICT|DELETE|TRUNCATE/);
});

test("lifecycle benchmark fixtures reject unsafe identities and populations", () => {
  assert.throws(
    () =>
      lifecycleFixtureStatements("id'; DROP TABLE principals;--", {
        expiredRows: 20_000,
        liveRows: 20_000,
      }),
    /identifier is invalid/,
  );
  for (const rows of [0, -1, 20_001, 1.5, "20000"])
    assert.throws(
      () =>
        lifecycleFixtureStatements("fe0403e5-3210-4321-8765-123456789abc", {
          expiredRows: rows,
          liveRows: 1,
        }),
      /outside safe bounds/,
    );
});
