import assert from "node:assert/strict";
import { test } from "node:test";
import { runArrivals } from "../../lib/benchmark-arrival-load.mjs";

const settings = {
  rate: 100,
  durationMs: 50,
  maxInFlight: 2,
  maxLatenessMs: 2,
};
const select = (index) => ({
  client: index % 2,
  epoch: "steady",
  expected: { active: false },
});
const inactive = { status: 200, body: { active: false } };

test("planned arrivals continue at the fixed rate when responses stall, with bounded concurrency", async () => {
  let now = 100,
    resolveRequest;
  const held = new Promise((resolve) => {
    resolveRequest = resolve;
  });
  const dispatched = [];
  const output = await runArrivals({
    settings,
    select,
    clock: () => now,
    sleep: async (ms) => {
      now += ms;
      if (now >= 150) resolveRequest(inactive);
    },
    perform: async (_, index) => {
      dispatched.push(index);
      return held;
    },
  });
  assert.deepEqual(dispatched, [0, 1]);
  assert.deepEqual(
    output.rows.map((r) => r.scheduledMs),
    [100, 110, 120, 130, 140],
  );
  assert.deepEqual(
    output.rows.map((r) => r.outcome),
    ["denied", "denied", "generator_full", "generator_full", "generator_full"],
  );
  assert.equal(output.peakInFlight, 2);
  assert.equal(output.wallMs, 50);
  assert.equal(output.rows[0].scheduledLatencyMs, 50);
});
test("timer stalls preserve every missed arrival without replaying a catch-up burst", async () => {
  let now = 0;
  const output = await runArrivals({
    settings,
    select,
    clock: () => now,
    sleep: async (ms) => {
      now += ms + 100;
    },
    perform: async () => inactive,
  });
  assert.equal(output.rows.length, 5);
  assert.equal(output.rows[0].outcome, "denied");
  assert.ok(
    output.rows
      .slice(1)
      .every((r) => r.outcome === "generator_late" && r.startMs === null),
  );
});
test("early timers are waited again; actual dispatch captures the current authorization epoch", async () => {
  let now = 0,
    early = true;
  const output = await runArrivals({
    settings: { ...settings, durationMs: 20 },
    clock: () => now,
    sleep: async (ms) => {
      now += early ? ms / 2 : ms;
      early = false;
    },
    select: () => ({
      client: 0,
      epoch: now < 10 ? "before" : "after",
      expected: { active: false },
    }),
    perform: async () => inactive,
  });
  assert.deepEqual(
    output.rows.map((r) => [r.startMs, r.epoch]),
    [
      [0, "before"],
      [10, "after"],
    ],
  );
});
test("transport failure is retained without raw errors or response secrets", async () => {
  let now = 0;
  const output = await runArrivals({
    settings,
    select,
    clock: () => now,
    sleep: async (ms) => {
      now += ms;
    },
    perform: async () => {
      throw new Error("secret");
    },
  });
  assert.ok(output.rows.every((r) => r.outcome === "transport_error"));
  assert.ok(!JSON.stringify(output).includes("secret"));
});

test("an interrupted schedule drains requests already sent before rejecting", async () => {
  let now = 0,
    release,
    returned = false,
    finished = false;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const running = runArrivals({
    settings,
    clock: () => now,
    sleep: async (ms) => {
      now += ms;
    },
    select: (index) => {
      if (index === 1) throw new Error("selection failed");
      return select(index);
    },
    perform: async () => {
      await held;
      finished = true;
      return inactive;
    },
  }).catch((error) => {
    returned = true;
    assert.equal(error.message, "selection failed");
  });
  for (let i = 0; i < 20; i++) await Promise.resolve();
  try {
    assert.equal(returned, false);
  } finally {
    release();
    await running;
  }
  assert.equal(finished, true);
});
test("unexpected response classification failures reject only after all dispatched work settles", async () => {
  let now = 0;
  await assert.rejects(
    runArrivals({
      settings,
      clock: () => now,
      sleep: async (ms) => {
        now += ms;
      },
      select: () => ({ client: 0, epoch: "steady", expected: undefined }),
      perform: async () => inactive,
    }),
    TypeError,
  );
  assert.equal(now, 50);
});
