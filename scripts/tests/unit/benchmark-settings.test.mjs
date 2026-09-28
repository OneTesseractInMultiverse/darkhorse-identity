import assert from "node:assert/strict";
import { test } from "node:test";
import { benchmarkSettings } from "../../lib/benchmark-settings.mjs";

test("benchmark settings preserve the five-connection default and ignore deployment settings", () => {
  const defaults = benchmarkSettings({});
  assert.equal(defaults.poolSize, 5);
  assert.equal(defaults.authorizationCacheEnabled, false);
  assert.equal(defaults.profile.name, "smoke");
  assert.deepEqual(
    benchmarkSettings({ DARKHORSE_DATABASE_POOL_SIZE: "32" }),
    defaults,
  );
});

test("pool comparisons accept the complete supported integer range with an explicit workload", () => {
  for (let size = 1; size <= 32; size++) {
    const settings = benchmarkSettings({
      BENCH_POOL_SIZE: String(size),
      BENCH_PROFILE: "profile-baseline",
      BENCH_AUTHORIZATION_CACHE_ENABLED: "true",
    });
    assert.equal(settings.poolSize, size);
    assert.equal(settings.authorizationCacheEnabled, true);
    assert.equal(settings.profile.profiling, true);
    assert.deepEqual(settings.profile.arrivals.rates, [200, 800, 1600]);
  }
});

test("invalid pool and workload settings fail without echoing input", () => {
  for (const value of [
    "",
    "0",
    "33",
    "-1",
    "1.5",
    "1e1",
    "05",
    " 5",
    "5\n",
    "NaN",
    "postgres://private:credential@example",
    null,
    5,
  ])
    assert.throws(() => benchmarkSettings({ BENCH_POOL_SIZE: value }), {
      message: "BENCH_POOL_SIZE must be an integer from 1 through 32.",
    });
  assert.throws(
    () => benchmarkSettings({ BENCH_PROFILE: "unknown" }),
    /Unknown benchmark profile/,
  );
  for (const value of ["", "1", "False", " true", "false\n", null])
    assert.throws(
      () => benchmarkSettings({ BENCH_AUTHORIZATION_CACHE_ENABLED: value }),
      { message: "BENCH_AUTHORIZATION_CACHE_ENABLED must be true or false." },
    );
});
