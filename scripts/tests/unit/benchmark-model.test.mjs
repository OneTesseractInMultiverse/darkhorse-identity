import assert from "node:assert/strict";
import { test } from "node:test";
import {
  benchmarkProfile,
  classify,
  summarize,
  phaseSummary,
  validateLoad,
} from "../../lib/benchmark-model.mjs";

test("benchmark profiles bound load and reject accidental unbounded settings", () => {
  assert.equal(benchmarkProfile("smoke").requests, 128);
  assert.equal(benchmarkProfile("baseline").requests, 2048);
  for (const suffix of ["smoke", "baseline"]) {
    const profiled = benchmarkProfile(`profile-${suffix}`);
    assert.deepEqual(profiled, {
      ...benchmarkProfile(`arrival-${suffix}`),
      name: `profile-${suffix}`,
      profiling: true,
    });
  }
  assert.throws(() => benchmarkProfile("production"), /profile/);
  assert.equal(benchmarkProfile("arrival-smoke").arrivals.durationMs, 2000);
  assert.equal(benchmarkProfile("arrival-baseline").clients, 8);
});
const expected = {
  active: true,
  iss: "https://issuer.example",
  aud: "resource",
  sub: "subject",
  client_id: "client",
  scope: "openid operate",
  capabilities: ["read", "write"],
};
const active = {
  status: 200,
  body: { ...expected, capabilities: ["write", "read"] },
};
test("successful timings require the exact live authority; stale and foreign responses fail", () => {
  assert.equal(classify(active, expected), "authorized");
  assert.equal(
    classify(active, {
      ...expected,
      capabilities: ["read"],
      alternatives: [["read", "write"]],
    }),
    "authorized",
  );
  assert.equal(
    classify(
      { status: 200, body: { active: false } },
      { ...expected, allowInactive: true },
    ),
    "denied",
  );
  assert.equal(
    classify(
      { status: 200, body: { active: false, sub: "leaked" } },
      { ...expected, allowInactive: true },
    ),
    "violation",
  );
  assert.equal(
    classify(
      { status: 200, body: { ...expected, capabilities: [1] } },
      expected,
    ),
    "violation",
  );
  for (const property of ["iss", "aud", "sub", "client_id", "scope"])
    assert.equal(
      classify(
        { ...active, body: { ...active.body, [property]: "foreign" } },
        expected,
      ),
      "violation",
    );
  assert.equal(
    classify(active, { ...expected, capabilities: ["read"] }),
    "violation",
  );
  assert.equal(
    classify({ status: 200, body: { active: false } }, expected),
    "violation",
  );
  assert.equal(
    classify({ status: 200, body: { active: false } }, { active: false }),
    "denied",
  );
  assert.equal(
    classify(
      { status: 200, body: { active: false, sub: "leaked" } },
      { active: false },
    ),
    "violation",
  );
  assert.equal(classify({ status: 401 }, { status: 401 }), "denied");
  assert.equal(classify(active, { status: 401 }), "violation");
  assert.equal(
    classify({ status: 200, body: { status: "ok" } }, { status: 200 }),
    "healthy",
  );
  for (const status of [429, 503])
    assert.equal(classify({ status }, expected), "unavailable");
  assert.equal(classify({ status: 403 }, expected), "error");
  assert.equal(classify({ status: 0 }, expected), "transport_error");
});
test("reports preserve denials and errors instead of inflating authorized throughput", () => {
  const rows = ["authorized", "denied", "unavailable", "violation"].map(
    (outcome, i) => ({ outcome, elapsedMs: i + 1 }),
  );
  const report = summarize(rows, 2000);
  assert.equal(report.authorizedPerSecond, 0.5);
  assert.equal(report.attemptsPerSecond, 2);
  assert.deepEqual(report.allLatencyMs, { p50: 2, p95: 4, p99: 4 });
  assert.deepEqual(report.authorizedLatencyMs, { p50: 1, p95: 1, p99: 1 });
  assert.equal(report.denialRate, 0.25);
  assert.equal(report.errorRate, 0.5);
  assert.equal(summarize([], 1).allLatencyMs, null);
  assert.equal(summarize([], 1).authorizedPerSecond, 0);
});

test("per-client summaries retain health/invalid traffic without changing the shared wall time", () => {
  const report = phaseSummary(
    "noise",
    32,
    [
      { client: 0, outcome: "authorized", elapsedMs: 10 },
      { client: 1, outcome: "denied", elapsedMs: 2 },
      { client: "health", outcome: "healthy", elapsedMs: 1 },
    ],
    1000,
    { startMs: 2, acknowledgedMs: 5 },
  );
  assert.equal(report.byClient[0].authorizedPerSecond, 1);
  assert.equal(report.byClient[1].authorizedLatencyMs, null);
  assert.equal(report.byClient.health.outcomes.healthy, 1);
  assert.deepEqual(report.change, { startMs: 2, acknowledgedMs: 5 });
  for (const [count, workers] of [
    [NaN, 1],
    [1.5, 1],
    [8193, 1],
    [1, 0],
    [1, 1.1],
  ])
    assert.throws(() => validateLoad(count, workers), /bounds/);
});
