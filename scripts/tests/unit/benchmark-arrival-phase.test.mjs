import assert from "node:assert/strict";
import { test } from "node:test";
import { measureArrivals } from "../../lib/benchmark-arrival-phase.mjs";

// Deterministic virtual timers: no wall clock, network, files or process settings.
function virtualTime() {
  let now = 0;
  const timers = [];
  const clock = () => now;
  const sleep = (ms) =>
    new Promise((resolve) => timers.push({ at: now + ms, resolve }));
  async function drive(promise) {
    let settled = false;
    void promise.then(
      () => {
        settled = true;
      },
      () => {
        settled = true;
      },
    );
    for (let steps = 0; steps < 100 && !settled; steps++) {
      for (let micro = 0; micro < 30; micro++) await Promise.resolve();
      if (!settled && timers.length) {
        now = Math.min(...timers.map((t) => t.at));
        for (let i = timers.length - 1; i >= 0; i--)
          if (timers[i].at <= now) timers.splice(i, 1)[0].resolve();
      }
    }
    assert.equal(settled, true, "virtual workload did not settle");
    return promise;
  }
  return { clock, sleep, drive };
}
const settings = {
  rate: 100,
  durationMs: 50,
  maxInFlight: 8,
  maxLatenessMs: 2,
};
function workload(time, select) {
  return {
    name: "change",
    settings,
    ...time,
    select,
    perform: async () => {
      await time.sleep(15);
      return { status: 200, body: { active: false } };
    },
  };
}
const selected = (epoch) => ({ client: 0, epoch, expected: { active: false } });

test("mutations commit halfway through paced traffic and later dispatches require fresh state", async () => {
  const time = virtualTime();
  let committed = false;
  const report = await time.drive(
    measureArrivals({
      ...workload(time, () => selected(committed ? "after" : "before")),
      change: async () => {
        await time.sleep(1);
        committed = true;
      },
    }),
  );
  assert.deepEqual(
    report.rows.map((r) => r.epoch),
    ["before", "before", "before", "after", "after"],
  );
  assert.deepEqual(report.summary.change, { startMs: 25, acknowledgedMs: 26 });
  assert.equal(report.summary.wallMs, 55);
  assert.equal(report.summary.attempts, 5);
});
test("a failed mutation drains the remaining arrivals before propagating failure", async () => {
  const time = virtualTime();
  let dispatched = 0;
  await assert.rejects(
    time.drive(
      measureArrivals({
        ...workload(time, () => {
          dispatched++;
          return selected("before");
        }),
        change: async () => {
          throw new Error("mutation failed");
        },
      }),
    ),
    /mutation failed/,
  );
  assert.equal(dispatched, 5);
  assert.equal(time.clock(), 55);
});
test("load failure waits for the concurrent mutation to settle; ordinary phases omit mutations", async () => {
  const time = virtualTime();
  let committed = false;
  await assert.rejects(
    time.drive(
      measureArrivals({
        ...workload(time, (index) => {
          if (index === 1) throw new Error("load failed");
          return selected("before");
        }),
        change: async () => {
          await time.sleep(1);
          committed = true;
        },
      }),
    ),
    /load failed/,
  );
  assert.equal(committed, true);
  const next = virtualTime();
  const result = await next.drive(
    measureArrivals(workload(next, () => selected("steady"))),
  );
  assert.equal(result.summary.change, null);
  assert.equal(result.summary.scheduled, 5);
});
