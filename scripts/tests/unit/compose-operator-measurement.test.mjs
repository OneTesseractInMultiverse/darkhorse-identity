import assert from "node:assert/strict";
import { test } from "node:test";
import {
  composeHttpStatusCounts,
  composeOperatorMeasurement,
} from "../../lib/compose-operator-measurement.mjs";

function phase(name) {
  return {
    name,
    summary: {
      offeredRate: 25,
      scheduled: 100,
      attempts: 98,
      outcomes: {
        authorized: 97,
        unavailable: 1,
        violation: 0,
      },
      generatorDrops: { late: 1, full: 1 },
      allScheduledLatencyMs: { p50: 8, p95: 15, p99: 25 },
      authorizedScheduledLatencyMs: { p50: 8, p95: 14, p99: 20 },
      peakInFlight: 3,
    },
    statusCounts: { 200: 97, 503: 1 },
  };
}

function command(worker, operation, result, startMs) {
  return {
    worker,
    operation,
    result,
    operationId: "sensitive-operation-id",
    scheduledMs: startMs - 3,
    startMs,
    endMs: startMs + 10,
    email: "private@example.com",
    password: "private-password",
  };
}

function observation(runtimeConnections) {
  return {
    runtimeConnections,
    activeRuntimeConnections: 2,
    waitingLocks: 0,
    apiCpu: "45.0%",
    apiMemory: "112MiB / 512MiB",
    databaseCpu: "20.0%",
    databaseMemory: "87MiB / 512MiB",
  };
}

const reads = [
  command(0, "account.show", "read", 10),
  command(0, "application.show", "read", 20),
  command(0, "account.show.missing", "not_found", 30),
  command(0, "client.show", "read", 40),
  command(1, "account.show.denied", "denied", 10),
  command(1, "application.show.denied", "denied", 20),
  command(1, "account.show.denied", "denied", 30),
  command(1, "client.show.denied", "denied", 40),
];

test("Compose summary preserves failures and reports actual bounded runtime samples", () => {
  const result = composeOperatorMeasurement({
    phases: [phase("before"), phase("overlap"), phase("after")],
    commands: reads,
    observations: [observation(3), observation(9)],
    before: observation(2),
    after: observation(2),
  });

  assert.equal(result.status, "completed");
  assert.equal(result.databaseRole, "darkhorse_runtime");
  assert.equal(result.httpPhases[1].outcomes.unavailable, 1);
  assert.equal(result.httpPhases[1].outcomes.generator_late, 1);
  assert.equal(result.httpPhases[1].outcomes.generator_full, 1);
  assert.deepEqual(result.httpPhases[1].httpStatuses, {
    200: 97,
    503: 1,
  });
  assert.deepEqual(result.operator.outcomes, {
    read: 3,
    not_found: 1,
    denied: 4,
  });
  assert.equal(result.runtime.maximumSampledConnections, 9);
  assert.equal(result.runtime.maximumSampledWaitingLocks, 0);
  assert.equal(result.runtime.before.runtimeConnections, 2);
  assert.equal(result.runtime.after.runtimeConnections, 2);
  const serialized = JSON.stringify(result);
  assert.doesNotMatch(
    serialized,
    /sensitive-operation-id|private@example|private-password/,
  );
});

test("Compose status summary retains exact bounded HTTP codes and skips drops", () => {
  assert.deepEqual(
    composeHttpStatusCounts([
      { status: 200 },
      { status: 503 },
      { status: null, outcome: "generator_late" },
    ]),
    { 200: 1, 503: 1 },
  );
  assert.throws(
    () => composeHttpStatusCounts([{ status: 99 }]),
    /HTTP status row/,
  );
  assert.throws(
    () =>
      composeHttpStatusCounts(
        Array.from({ length: 30001 }, () => ({ status: 200 })),
      ),
    /HTTP status rows/,
  );
});

test("Compose summary rejects unknown operation labels, unsafe counts, and oversized traces", () => {
  assert.throws(
    () =>
      composeOperatorMeasurement({
        phases: [phase("before"), phase("overlap"), phase("after")],
        commands: [command(0, "account.delete", "read", 10)],
        observations: [],
      }),
    /operator command/,
  );
  assert.throws(
    () =>
      composeOperatorMeasurement({
        phases: [
          phase("before"),
          phase("overlap"),
          phase("after"),
          phase("extra"),
        ],
        commands: [],
        observations: [],
      }),
    /HTTP phase/,
  );
  assert.throws(
    () =>
      composeOperatorMeasurement({
        phases: [],
        commands: [],
        observations: [observation(-1)],
      }),
    /runtime observation/,
  );
});
