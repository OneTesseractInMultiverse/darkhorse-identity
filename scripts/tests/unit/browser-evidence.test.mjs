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
  assert.equal(events[0], "DARKHORSE_BROWSER:email:started");
  assert.ok(events[1].startsWith("DARKHORSE_DIAGNOSTIC:"));
  assert.ok(!events[1].includes("private fixture details"));
  assert.equal(events.length, 2);
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

test("diagnostics keep source locations while discarding messages, URLs and assertion values", async () => {
  const { browserFailure } = await import("../../lib/browser-evidence.mjs");
  const error = {
    name: "TimeoutError",
    stack:
      "TimeoutError: private-token https://private.example/code=secret\n    at verifyProvider (/repo/scripts/lib/provider-browser.mjs:215:13)\n    at browserPhase (file:///repo/scripts/lib/browser-evidence.mjs:64:22)\n    at hidden (/other/private.mjs:1:2)\n",
  };
  assert.deepEqual(browserFailure(error, "/repo"), {
    type: "TimeoutError",
    locations: [
      { file: "scripts/lib/provider-browser.mjs", line: 215, column: 13 },
      { file: "scripts/lib/browser-evidence.mjs", line: 64, column: 22 },
    ],
  });
  assert.deepEqual(
    browserFailure({ name: "private-value", stack: "private-value" }, "/repo"),
    { type: "Error", locations: [] },
  );
  assert.ok(!JSON.stringify(browserFailure(error, "/repo")).includes("secret"));
});

test("reports accept only bounded structured diagnostics and never turn a recorded failure into success", () => {
  const diagnostic = {
    type: "AssertionError",
    locations: [
      { file: "scripts/lib/provider-browser.mjs", line: 215, column: 13 },
    ],
    private: "secret",
  };
  const report = browserEvidence(
    `DARKHORSE_BROWSER:setup:started\nDARKHORSE_DIAGNOSTIC:${JSON.stringify(diagnostic)}\n`,
  );
  assert.deepEqual(report.diagnostic, {
    type: "AssertionError",
    locations: diagnostic.locations,
  });
  assert.ok(!JSON.stringify(report).includes("secret"));
  for (const invalid of [
    "broken",
    JSON.stringify({
      type: "Error",
      locations: [{ file: "../../secret", line: 1, column: 1 }],
    }),
    "x".repeat(1025),
    JSON.stringify({ type: "private", locations: [] }),
  ]) {
    const result = browserEvidence(`DARKHORSE_DIAGNOSTIC:${invalid}`);
    assert.equal(result.status, "incomplete");
    assert.equal(result.diagnostic, undefined);
  }
});
