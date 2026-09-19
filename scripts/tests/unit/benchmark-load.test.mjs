import assert from "node:assert/strict";
import { test } from "node:test";
import { runLoad } from "../../lib/benchmark-load.mjs";
test("load runner bounds in-flight work, retains failures, and captures expectations at dispatch", async () => {
  let tick = 0,
    inFlight = 0,
    peak = 0,
    reduced = false;
  const results = await runLoad({
    count: 20,
    concurrency: 3,
    clock: () => tick++,
    select: (index) => ({
      client: index % 2,
      epoch: reduced ? "after" : "before",
      expected: { active: false },
    }),
    perform: async (_, index) => {
      inFlight++;
      peak = Math.max(peak, inFlight);
      await Promise.resolve();
      inFlight--;
      reduced = true;
      if (index === 0) throw new Error("secret must never enter results");
      return {
        status: 200,
        body: { active: false },
        headers: { secret: "hidden" },
      };
    },
  });
  assert.equal(peak, 3);
  assert.equal(results.rows.length, 20);
  assert.deepEqual(
    results.rows.slice(0, 3).map((r) => r.epoch),
    ["before", "before", "before"],
  );
  assert.ok(results.rows.slice(3).every((r) => r.epoch === "after"));
  assert.equal(results.rows[0].outcome, "transport_error");
  assert.ok(!JSON.stringify(results).includes("secret"));
  assert.ok(results.wallMs > 0);
  await assert.rejects(runLoad({ count: 0, concurrency: 1 }), /bounds/);
  await assert.rejects(runLoad({ count: 10, concurrency: 129 }), /bounds/);
});

test("all started workers settle before a selection failure leaves the load runner", async () => {
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  let finished = false,
    returned = false;
  const running = runLoad({
    count: 2,
    concurrency: 2,
    clock: () => 1,
    select: (index) => {
      if (index === 1) throw new Error("invalid selection");
      return { client: 0, epoch: "steady", expected: { active: false } };
    },
    perform: async () => {
      await held;
      finished = true;
      return { status: 200, body: { active: false } };
    },
  }).catch((error) => {
    returned = true;
    assert.equal(error.message, "invalid selection");
  });
  for (let i = 0; i < 10; i++) await Promise.resolve();
  try {
    assert.equal(returned, false);
  } finally {
    release();
    await running;
  }
  assert.equal(finished, true);
});
