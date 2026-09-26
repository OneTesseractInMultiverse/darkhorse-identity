import assert from "node:assert/strict";
import { test } from "node:test";
import { benchmarkProfile } from "../../lib/benchmark-model.mjs";
import {
  operatorSummary,
  operatorLimits,
  operatorResult,
  operatorOperations,
  operatorRead,
} from "../../lib/benchmark-operator-model.mjs";
import { measureOperatorPhase } from "../../lib/benchmark-operator-load.mjs";

test("detail profiles keep bounded authenticated work under runtime database grants", () => {
  for (const suffix of ["smoke", "baseline"]) {
    const detail = benchmarkProfile(`operator-detail-${suffix}`);
    const list = benchmarkProfile(`operator-${suffix}`);
    assert.equal(detail.operators, true);
    assert.equal(detail.details, true);
    assert.equal(detail.restrictedDatabase, true);
    assert.deepEqual(detail.arrivals, list.arrivals);
    assert.equal(detail.clients, list.clients);
  }
  assert.deepEqual(operatorOperations(true), [
    "account.show",
    "application.show",
    "account.show",
    "client.show",
  ]);
  assert.deepEqual(operatorOperations(false), [
    "account.list",
    "application.list",
    "account.list",
    "application.list",
  ]);
  const ids = {
    principal: "principal",
    application: "application",
    client: "client",
  };
  assert.deepEqual(operatorRead("account.show", ids), {
    args: ["account", "show", "principal"],
    table: "operator_account_audit",
    result: "read",
  });
  assert.deepEqual(operatorRead("application.show", ids), {
    args: ["application", "show", "application"],
    table: "operator_catalog_detail_audit",
    result: "read",
  });
  assert.deepEqual(operatorRead("client.show", ids), {
    args: ["client", "show", "application", "client"],
    table: "operator_catalog_detail_audit",
    result: "read",
  });
  assert.throws(() => operatorRead("unknown", ids));
  assert.equal(operatorLimits(5, true).databaseRole, "darkhorse_runtime");
});

test("operator profiles bound independent processes and preserve comparable offered load", () => {
  for (const [suffix, durationMs] of [
    ["smoke", 3000],
    ["baseline", 10000],
  ]) {
    const profile = benchmarkProfile(`operator-${suffix}`);
    assert.equal(profile.operators, true);
    assert.equal(profile.arrivals.durationMs, durationMs);
    assert.equal(profile.arrivals.rate, 200);
    assert.equal(profile.clients, suffix === "smoke" ? 4 : 8);
  }
});
test("operator summaries count dispatched requests once across overlapping processes", () => {
  const commands = [
    { startMs: 10, endMs: 20 },
    { startMs: 15, endMs: 25 },
  ];
  const rows = [null, 0, 10, 19, 25].map((startMs) => ({
    startMs,
    elapsedMs: 1,
    scheduledLatencyMs: 2,
    outcome: "authorized",
  }));
  const summary = operatorSummary(commands, rows);
  assert.equal(summary.requestsDuringCommands, 2);
  assert.deepEqual(summary.authorizedScheduledLatencyDuringCommandsMs, {
    p50: 2,
    p95: 2,
    p99: 2,
  });
  assert.deepEqual(summary.allLatencyDuringCommandsMs, {
    p50: 1,
    p95: 1,
    p99: 1,
  });
  assert.deepEqual(
    summary.commands.map((row) => row.requestsDuring),
    [2, 1],
  );
  assert.deepEqual(summary.commandLatencyMs, { p50: 10, p95: 10, p99: 10 });
  assert.equal(operatorSummary([], rows).commandLatencyMs, null);
});

test("overlap latency keeps failed responses visible without counting them as useful authorization", () => {
  const commands = [
    { operation: "account.show", scheduledMs: 0, startMs: 0, endMs: 10 },
  ];
  const rows = [
    { startMs: 1, elapsedMs: 3, scheduledLatencyMs: 4, outcome: "authorized" },
    {
      startMs: 2,
      elapsedMs: 100,
      scheduledLatencyMs: 101,
      outcome: "unavailable",
    },
    { startMs: 3, elapsedMs: 200, scheduledLatencyMs: 201, outcome: "denied" },
    {
      startMs: null,
      elapsedMs: 0,
      scheduledLatencyMs: 0,
      outcome: "generator_drop",
    },
  ];
  const summary = operatorSummary(commands, rows);
  assert.equal(summary.requestsDuringCommands, 3);
  assert.equal(summary.commands[0].requestsDuring, 3);
  assert.equal(summary.allLatencyDuringCommandsMs.p95, 200);
  assert.deepEqual(summary.authorizedScheduledLatencyDuringCommandsMs, {
    p50: 4,
    p95: 4,
    p99: 4,
  });
  assert.equal(
    operatorSummary(commands, rows.slice(1))
      .authorizedScheduledLatencyDuringCommandsMs,
    null,
  );
});
test("bounded CLI envelopes project only successful audit identifiers", () => {
  const operation_id = "00000000-0000-4000-8000-000000000001";
  assert.deepEqual(
    operatorResult(
      JSON.stringify({
        ok: true,
        schema_version: 1,
        data: { operation_id, secret: "private" },
      }),
    ),
    { operationId: operation_id },
  );
  for (const value of [
    "secret",
    "null",
    "{}",
    JSON.stringify({ ok: false, schema_version: 1, data: { operation_id } }),
    JSON.stringify({ ok: true, schema_version: 2, data: { operation_id } }),
    JSON.stringify({ ok: true, schema_version: 1, data: null }),
    JSON.stringify({ ok: true, schema_version: 1, data: {} }),
    JSON.stringify({
      ok: true,
      schema_version: 1,
      data: { operation_id: "private" },
    }),
  ])
    assert.throws(() => operatorResult(value), {
      message: "Invalid benchmark operator response.",
    });
});
test("operator lanes never overlap their own commands and drain before returning", async () => {
  const counts = [0, 0],
    active = [false, false];
  let now = 0;
  const result = await measureOperatorPhase({
    durationMs: 40,
    clock: () => now,
    sleep: async (ms) => {
      now += ms;
    },
    load: async () => ({
      rows: [{ startMs: 0 }],
      summary: { name: "fixture" },
    }),
    invoke: async (worker, operation) => {
      assert.equal(active[worker], false);
      active[worker] = true;
      assert.equal(
        operation,
        counts[worker] % 2 === 0 ? "account.list" : "application.list",
      );
      counts[worker]++;
      await Promise.resolve();
      now++;
      active[worker] = false;
      return { operationId: "fixture" };
    },
  });
  assert.deepEqual(counts, [4, 4]);
  assert.equal(result.summary.operators.commands.length, 8);
  assert.deepEqual(active, [false, false]);
});
test("CLI failure stops its lane without retry and drains the other lane and HTTP", async () => {
  const counts = [0, 0];
  let drained = false,
    now = 0;
  await assert.rejects(
    measureOperatorPhase({
      durationMs: 4,
      clock: () => now,
      sleep: async (ms) => {
        now += ms;
      },
      load: async () => {
        for (let i = 0; i < 30; i++) await Promise.resolve();
        drained = true;
        return { rows: [], summary: {} };
      },
      invoke: async (worker) => {
        counts[worker]++;
        if (worker === 0) throw new Error("command failed");
        return {};
      },
    }),
    /command failed/,
  );
  assert.deepEqual(counts, [1, 4]);
  assert.equal(drained, true);
});
test("HTTP failure still drains both CLI lanes before cleanup", async () => {
  let finished = 0,
    now = 0;
  await assert.rejects(
    measureOperatorPhase({
      durationMs: 4,
      clock: () => now,
      sleep: async (ms) => {
        now += ms;
      },
      load: async () => {
        throw new Error("load failed");
      },
      invoke: async () => {
        await Promise.resolve();
        finished++;
        return {};
      },
    }),
    /load failed/,
  );
  assert.equal(finished, 8);
});

test("configured CLI pool envelopes account for the smaller deployment pool", () => {
  assert.equal(operatorLimits(1).configuredReadPhaseDatabaseEnvelope, 3);
  assert.equal(operatorLimits(5).configuredReadPhaseDatabaseEnvelope, 9);
  assert.equal(operatorLimits(32).databaseConnectionsPerCli, 2);
});

test("detail lanes report each operation and preserve scheduled queue delay", async () => {
  let now = 0;
  const seen = [[], []];
  const result = await measureOperatorPhase({
    details: true,
    durationMs: 40,
    clock: () => now,
    sleep: async (ms) => {
      now += ms;
    },
    load: async () => ({ rows: [], summary: {} }),
    invoke: async (worker, operation) => {
      seen[worker].push(operation);
      now += 20;
      return { operationId: "fixture" };
    },
  });
  for (const operations of seen)
    assert.deepEqual(operations, operatorOperations(true));
  assert.equal(
    result.summary.operators.byOperation["account.show"].commands,
    4,
  );
  assert.equal(result.summary.operators.byOperation["client.show"].commands, 2);
  assert.ok(
    result.summary.operators.commandScheduledLatencyMs.p95 >
      result.summary.operators.commandLatencyMs.p95,
  );
});

// Source-defined virtual clock; no real timers or process settings.
function virtualTime() {
  let now = 0;
  const timers = [];
  const clock = () => now;
  const sleep = (ms) =>
    new Promise((resolve) => timers.push({ at: now + ms, resolve }));
  async function drive(promise) {
    let settled = false;
    void promise.finally(() => {
      settled = true;
    });
    for (let step = 0; step < 100 && !settled; step++) {
      for (let micro = 0; micro < 30; micro++) await Promise.resolve();
      if (!settled && timers.length) {
        now = Math.min(...timers.map((timer) => timer.at));
        for (let i = timers.length - 1; i >= 0; i--)
          if (timers[i].at <= now) timers.splice(i, 1)[0].resolve();
      }
    }
    assert.equal(settled, true);
    return promise;
  }
  return { clock, sleep, drive };
}
test("paced CLI lanes retain planned times but never start a new command before completion", async () => {
  for (const cost of [2, 15]) {
    const time = virtualTime();
    const result = await time.drive(
      measureOperatorPhase({
        ...time,
        durationMs: 40,
        load: async () => {
          await time.sleep(40);
          return { rows: [], summary: {} };
        },
        invoke: async () => {
          await time.sleep(cost);
          return {};
        },
      }),
    );
    for (const worker of [0, 1]) {
      const rows = result.summary.operators.commands.filter(
        (row) => row.worker === worker,
      );
      assert.deepEqual(
        rows.map((row) => row.scheduledMs),
        [0, 10, 20, 30],
      );
      assert.deepEqual(
        rows.map((row) => row.startMs),
        cost === 2 ? [0, 10, 20, 30] : [0, 15, 30, 45],
      );
      assert.equal(rows.at(-1).endMs, cost === 2 ? 32 : 60);
    }
  }
});

test("a timer that wakes early cannot start a command before its scheduled time", async () => {
  let now = 0;
  const result = await measureOperatorPhase({
    durationMs: 40,
    clock: () => now,
    sleep: async (ms) => {
      now += Math.min(ms, 1);
    },
    load: async () => ({ rows: [], summary: {} }),
    invoke: async () => ({}),
  });
  assert.ok(
    result.summary.operators.commands.every(
      (row) => row.startMs >= row.scheduledMs,
    ),
  );
});
