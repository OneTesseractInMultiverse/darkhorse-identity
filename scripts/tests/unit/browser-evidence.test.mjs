import test from "node:test";
import assert from "node:assert/strict";
import {
  browserPhase,
  browserMarker,
  browserEvidence,
} from "../../lib/browser-evidence.mjs";

test("phase completion follows successful work and preserves its result", async (t) => {
  const events = [];
  t.mock.method(console, "log", (value) => events.push(value));
  const result = await browserPhase("catalog", async () => {
    events.push("work");
    return 12;
  });
  assert.equal(result, 12);
  assert.deepEqual(events, [
    "DARKHORSE_BROWSER:catalog:started",
    "work",
    "DARKHORSE_BROWSER:catalog:passed",
  ]);
});
test("failed work never emits a pass and invalid phases never start work", async (t) => {
  const events = [];
  t.mock.method(console, "log", (value) => events.push(value));
  await assert.rejects(
    browserPhase("email", async () => {
      throw new Error("private fixture details");
    }),
  );
  assert.deepEqual(events, ["DARKHORSE_BROWSER:email:started"]);
  let invoked = false;
  await assert.rejects(
    browserPhase("private-token", async () => {
      invoked = true;
    }),
  );
  assert.equal(invoked, false);
  assert.throws(() => browserMarker("email", "arbitrary"));
  assert.deepEqual(browserEvidence("private-token"), {
    status: "incomplete",
    phase: "not started",
  });
});
