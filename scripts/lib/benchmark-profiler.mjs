import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { profileSql, quietSql, resetSql } from "./benchmark-profile-sql.mjs";

export async function profiler({ docker, db, profileSnapshot, wait = delay }) {
  assert.equal(typeof profileSnapshot, "function", "Profiling build required.");
  const sql = async (statement) =>
    (
      await docker([
        "exec",
        db.name,
        "psql",
        "-U",
        "postgres",
        "-d",
        "browser_test",
        "-v",
        "ON_ERROR_STOP=1",
        "-qAtc",
        statement,
      ])
    ).stdout.trim();
  await sql("CREATE EXTENSION pg_stat_statements");
  const settings = JSON.parse(
    await sql(
      `SELECT json_build_object('preload', current_setting('shared_preload_libraries'), 'tracking', current_setting('pg_stat_statements.track'), 'planning', current_setting('pg_stat_statements.track_planning'), 'utility', current_setting('pg_stat_statements.track_utility'));`,
    ),
  );
  assert.equal(settings.preload, "pg_stat_statements");
  assert.equal(settings.tracking, "top");
  assert.equal(settings.planning, "off");
  assert.equal(settings.utility, "on");
  const quiet = async () => {
    for (let attempt = 0; attempt < 100; attempt++) {
      if ((await sql(quietSql)) === "t") return;
      await wait(25);
    }
    throw new Error("Benchmark database did not become quiescent.");
  };
  let baseline;
  async function before() {
    await quiet();
    await profileSnapshot();
    await sql(resetSql);
    baseline = JSON.parse(await sql(profileSql));
  }
  async function after() {
    await quiet();
    const rust = await profileSnapshot();
    const observed = JSON.parse(await sql(profileSql));
    assert.equal(
      observed.deallocations,
      baseline.deallocations,
      "Statement statistics evicted during phase.",
    );
    // PostgreSQL-produced LSNs only; validate before inserting into a SQL literal.
    for (const lsn of [baseline.wal_lsn, observed.wal_lsn])
      assert.match(lsn, /^[0-9A-F]+\/[0-9A-F]+$/);
    const walBytes = await sql(
      `SELECT pg_wal_lsn_diff('${observed.wal_lsn}', '${baseline.wal_lsn}')::text`,
    );
    assert.match(walBytes, /^\d+$/);
    return {
      rust,
      sql: { groups: observed.groups, clusterWalBytes: walBytes, evictions: 0 },
    };
  }
  return { before, after, settings };
}
