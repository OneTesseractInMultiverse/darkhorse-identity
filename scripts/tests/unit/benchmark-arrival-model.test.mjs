import assert from "node:assert/strict";
import { test } from "node:test";
import {
  arrivalPlan,
  arrivalAction,
  arrivalSummary,
} from "../../lib/benchmark-arrival-model.mjs";

test("arrival plans bound duration, rate, request count and in-flight work", () => {
  assert.deepEqual(
    arrivalPlan({
      rate: 200,
      durationMs: 1000,
      maxInFlight: 8,
      maxLatenessMs: 5,
    }),
    {
      count: 200,
      intervalMs: 5,
      durationMs: 1000,
      maxInFlight: 8,
      maxLatenessMs: 5,
      rate: 200,
    },
  );
  for (const invalid of [
    { rate: 0 },
    { rate: NaN },
    { rate: 2001 },
    { durationMs: 0 },
    { durationMs: 30001 },
    { rate: 2000, durationMs: 30000 },
    { maxInFlight: 0 },
    { maxInFlight: 129 },
    { maxLatenessMs: -1 },
    { maxLatenessMs: 101 },
  ])
    assert.throws(
      () =>
        arrivalPlan({
          rate: 200,
          durationMs: 1000,
          maxInFlight: 8,
          maxLatenessMs: 5,
          ...invalid,
        }),
      /bounds/,
    );
});
test("late arrivals are discarded and capacity never causes a hidden request queue", () => {
  const plan = arrivalPlan({
    rate: 200,
    durationMs: 1000,
    maxInFlight: 8,
    maxLatenessMs: 5,
  });
  assert.equal(
    arrivalAction(plan, {
      scheduledMs: 10,
      nowMs: 10,
      endMs: 1000,
      inFlight: 7,
    }),
    "dispatch",
  );
  assert.equal(
    arrivalAction(plan, {
      scheduledMs: 10,
      nowMs: 15,
      endMs: 1000,
      inFlight: 7,
    }),
    "dispatch",
  );
  assert.equal(
    arrivalAction(plan, {
      scheduledMs: 10,
      nowMs: 16,
      endMs: 1000,
      inFlight: 0,
    }),
    "generator_late",
  );
  assert.equal(
    arrivalAction(plan, {
      scheduledMs: 999,
      nowMs: 1000,
      endMs: 1000,
      inFlight: 0,
    }),
    "generator_late",
  );
  assert.equal(
    arrivalAction(plan, {
      scheduledMs: 10,
      nowMs: 10,
      endMs: 1000,
      inFlight: 8,
    }),
    "generator_full",
  );
});
test("scheduled latency includes driver delay while dropped arrivals cannot inflate throughput", () => {
  const plan = arrivalPlan({
    rate: 4,
    durationMs: 1000,
    maxInFlight: 8,
    maxLatenessMs: 5,
  });
  const rows = [
    {
      client: 0,
      outcome: "authorized",
      startMs: 2,
      scheduledMs: 0,
      elapsedMs: 10,
      dispatchDelayMs: 2,
      scheduledLatencyMs: 12,
    },
    {
      client: 1,
      outcome: "unavailable",
      startMs: 251,
      scheduledMs: 250,
      elapsedMs: 1,
      dispatchDelayMs: 1,
      scheduledLatencyMs: 2,
    },
    {
      client: 1,
      outcome: "generator_late",
      startMs: null,
      scheduledMs: 500,
      elapsedMs: null,
      dispatchDelayMs: null,
      scheduledLatencyMs: null,
    },
    {
      client: 0,
      outcome: "generator_full",
      startMs: null,
      scheduledMs: 750,
      elapsedMs: null,
      dispatchDelayMs: null,
      scheduledLatencyMs: null,
    },
  ];
  const report = arrivalSummary("paced", plan, {
    rows,
    wallMs: 1200,
    peakInFlight: 2,
  });
  assert.equal(report.scheduled, 4);
  assert.equal(report.attempts, 2);
  assert.equal(report.outcomes.unavailable, 1);
  assert.deepEqual(report.generatorDrops, { late: 1, full: 1, fraction: 0.5 });
  assert.equal(report.authorizedPerSecond, 1 / 1.2);
  assert.equal(report.scheduledFailureRate, 0.75);
  assert.deepEqual(report.authorizedScheduledLatencyMs, {
    p50: 12,
    p95: 12,
    p99: 12,
  });
  assert.deepEqual(report.dispatchDelayMs, { p50: 1, p95: 2, p99: 2 });
  assert.equal(report.byClient[1].scheduled, 2);
  assert.equal(report.byClient[1].scheduledPerSecond, 2);
  assert.equal(report.byClient[0].authorizedPerSecond, 1 / 1.2);
  const dropped = arrivalSummary("none", plan, {
    rows: rows.slice(2),
    wallMs: 1000,
    peakInFlight: 0,
  });
  assert.equal(dropped.allLatencyMs, null);
  assert.equal(dropped.scheduledFailureRate, 1);
});

test("empty or completely unsent populations have no invented latency or successful throughput", () => {
  const plan = arrivalPlan({
    rate: 1,
    durationMs: 1000,
    maxInFlight: 1,
    maxLatenessMs: 0,
  });
  const empty = arrivalSummary("empty", plan, {
    rows: [],
    wallMs: 1000,
    peakInFlight: 0,
  });
  assert.equal(empty.generatorDrops.fraction, 0);
  assert.equal(empty.scheduledFailureRate, 0);
  assert.equal(empty.authorizedPerSecond, 0);
  assert.equal(empty.authorizedScheduledLatencyMs, null);
});
