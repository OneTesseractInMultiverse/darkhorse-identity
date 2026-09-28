import assert from "node:assert/strict";
import { test } from "node:test";
import { composeOperatorProfile } from "../../lib/compose-operator-plan.mjs";

test("Compose operator smoke stays a small bounded readiness workload", () => {
  assert.deepEqual(composeOperatorProfile("smoke"), {
    name: "smoke",
    rate: 25,
    durationMs: 4000,
    maxInFlight: 32,
    maxLatenessMs: 25,
  });
});

test("Compose operator baseline keeps 200/s below the single-client budget", () => {
  assert.deepEqual(composeOperatorProfile("baseline"), {
    name: "baseline",
    rate: 200,
    durationMs: 8000,
    maxInFlight: 32,
    maxLatenessMs: 25,
  });
  assert.equal(200 * 8 * 3, 4800);
  assert.ok(200 * 8 * 3 < 6000);
});

test("Compose operator profiles are returned as caller-owned settings", () => {
  const profile = composeOperatorProfile("baseline");
  profile.rate = 2000;
  assert.equal(composeOperatorProfile("baseline").rate, 200);
  assert.throws(
    () => composeOperatorProfile("unbounded"),
    /Unknown Compose operator benchmark profile/,
  );
});
