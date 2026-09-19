import assert from "node:assert/strict";
import { test } from "node:test";
import { profiler } from "../../lib/benchmark-profiler.mjs";
import {
  profileSql,
  quietSql,
  resetSql,
} from "../../lib/benchmark-profile-sql.mjs";
function fixture({
  busy = 0,
  eviction = false,
  lsn = "0/1200",
  wal = "512",
  planning = "off",
} = {}) {
  const calls = [];
  let observations = 0,
    profileCalls = 0;
  const options = {
    db: { name: "owned-disposable-fixture" },
    wait: async () => {
      calls.push("wait");
    },
    profileSnapshot: async () => {
      calls.push("rust");
      return { sequence: ++profileCalls };
    },
    docker: async (args) => {
      assert.equal(args[1], "owned-disposable-fixture");
      const statement = args.at(-1);
      calls.push(statement);
      if (statement === quietSql) return { stdout: busy-- > 0 ? "f" : "t" };
      if (statement === profileSql)
        return {
          stdout: JSON.stringify({
            groups: [{ category: "clock", calls: 10 }],
            wal_lsn: ++observations === 1 ? "0/1000" : lsn,
            deallocations: eviction ? String(observations) : "0",
          }),
        };
      if (statement.includes("pg_wal_lsn_diff")) return { stdout: wal };
      if (statement.includes("current_setting"))
        return {
          stdout: JSON.stringify({
            preload: "pg_stat_statements",
            tracking: "top",
            planning,
            utility: "on",
          }),
        };
      return { stdout: "" };
    },
  };
  return { calls, options };
}
test("phase profiling waits for rollback cleanup, resets before load and keeps WAL separate", async () => {
  const { calls, options } = fixture({ busy: 1 });
  const observer = await profiler(options);
  await observer.before();
  const report = await observer.after();
  assert.deepEqual(report, {
    rust: { sequence: 2 },
    sql: {
      groups: [{ category: "clock", calls: 10 }],
      clusterWalBytes: "512",
      evictions: 0,
    },
  });
  assert.deepEqual(calls.slice(2, 8), [
    quietSql,
    "wait",
    quietSql,
    "rust",
    resetSql,
    profileSql,
  ]);
});
test("evictions, untrusted LSNs, bad settings and missing instrumented builds fail collection", async () => {
  for (const setting of [
    { eviction: true },
    { lsn: "0/1200'; SELECT secret" },
    { wal: "-1" },
  ]) {
    const { options, calls } = fixture(setting);
    const observer = await profiler(options);
    await observer.before();
    await assert.rejects(observer.after());
    assert.ok(calls.every((statement) => !statement.includes("SELECT secret")));
  }
  await assert.rejects(profiler(fixture({ planning: "on" }).options));
  await assert.rejects(profiler({ profileSnapshot: undefined }));
});
test("a nonquiescent application cannot silently contaminate the next phase", async () => {
  const { options, calls } = fixture({ busy: 100 });
  const observer = await profiler(options);
  await assert.rejects(observer.before(), /quiescent/);
  assert.equal(calls.filter((c) => c === "wait").length, 100);
  assert.ok(!calls.includes(resetSql));
});
