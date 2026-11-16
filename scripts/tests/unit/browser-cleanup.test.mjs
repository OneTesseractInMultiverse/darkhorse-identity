import { test } from "node:test";
import assert from "node:assert/strict";
import { closeBrowserResources } from "../../lib/browser-cleanup.mjs";

test("browser cleanup attempts every owned resource even when earlier closes fail", async () => {
  const events = [];
  const failures = [new Error("browser close"), new Error("receiver close")];
  await assert.rejects(
    closeBrowserResources([
      async () => {
        events.push("browser");
        throw failures[0];
      },
      async () => {
        events.push("server");
      },
      async () => {
        events.push("receiver");
        throw failures[1];
      },
      async () => {
        events.push("tls");
      },
    ]),
    (error) =>
      error instanceof AggregateError &&
      error.errors.length === 2 &&
      error.errors.every((value, i) => value === failures[i]),
  );
  assert.deepEqual(events, ["browser", "server", "receiver", "tls"]);
});

test("cleanup preserves the exercise failure alongside teardown failures", async () => {
  const original = new Error("exercise failed");
  const teardown = new Error("teardown failed");
  await assert.rejects(
    closeBrowserResources(
      [
        async () => {
          throw teardown;
        },
      ],
      original,
    ),
    (error) =>
      error instanceof AggregateError &&
      error.errors[0] === original &&
      error.errors[1] === teardown,
  );
  await closeBrowserResources([async () => {}], original);
  await closeBrowserResources([]);
});

test("partial setup and a failed server stop still reach the receiver and proxy", async () => {
  let browser;
  const events = [];
  await assert.rejects(
    closeBrowserResources([
      () => browser?.close(),
      async () => {
        events.push("server");
        throw new Error("stop failed");
      },
      async () => {
        events.push("receiver");
      },
      async () => {
        events.push("proxy");
      },
    ]),
    AggregateError,
  );
  assert.deepEqual(events, ["server", "receiver", "proxy"]);
});
