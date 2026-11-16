import { test } from "node:test";
import assert from "node:assert/strict";
import {
  activatePublishedKey,
  publicationWait,
} from "../../lib/logout-jwks-signing.mjs";

const staged = {
  kid: "k".repeat(43),
  phase: "staged",
  revision: 3,
  recorded_outcome: "completed",
};
test("publication cancellation interrupts the wait and prevents activation", async () => {
  const abort = new AbortController();
  let started;
  const waiting = new Promise((resolve) => {
    started = resolve;
  });
  let invoked = false;
  const sleep = async (_ms, _value, { signal } = {}) => {
    started(signal);
    if (!signal) return;
    await new Promise((resolve, reject) => {
      signal.addEventListener("abort", () => reject(signal.reason), {
        once: true,
      });
    });
  };
  const pending = activatePublishedKey(
    async () => {
      invoked = true;
      return {
        stdout: JSON.stringify({ ...staged, phase: "active", revision: 4 }),
      };
    },
    staged,
    { now: () => 0, publishedAt: 0, sleep, signal: abort.signal },
  );
  const rejection = assert.rejects(pending);
  assert.equal(await waiting, abort.signal);
  abort.abort();
  await rejection;
  assert.equal(invoked, false);
  await assert.rejects(
    activatePublishedKey(
      async () => {
        invoked = true;
      },
      staged,
      { now: () => 100000, publishedAt: 0, signal: abort.signal },
    ),
  );
  assert.equal(invoked, false);
});
test("publication waits use elapsed monotonic time without shortening the provider's minute", () => {
  assert.equal(publicationWait(100, 100), 60100);
  assert.equal(publicationWait(100, 60099), 101);
  assert.equal(publicationWait(100, 60200), 0);
  for (const times of [
    [100, 99],
    [NaN, 100],
    [0, Infinity],
  ])
    assert.throws(() => publicationWait(...times));
});
test("activation follows the real wait, keeps the revision fence, and never retries a mutation", async () => {
  const events = [];
  let now = 100;
  const invoke = async (args) => {
    events.push(args);
    assert.equal(now, 60200);
    return {
      stdout: JSON.stringify({ ...staged, phase: "active", revision: 4 }),
    };
  };
  const options = {
    now: () => now,
    sleep: async (ms) => {
      events.push(ms);
      now += ms;
    },
    publishedAt: 100,
  };
  assert.equal(
    (await activatePublishedKey(invoke, staged, options)).phase,
    "active",
  );
  assert.deepEqual(events, [60100, ["signing-activate", staged.kid, "3"]]);
  let attempts = 0;
  await assert.rejects(
    activatePublishedKey(
      async () => {
        attempts++;
        throw new Error("uncertain commit");
      },
      staged,
      options,
    ),
  );
  assert.equal(attempts, 1);
  await assert.rejects(
    activatePublishedKey(invoke, { ...staged, phase: "active" }, options),
  );
  await assert.rejects(
    activatePublishedKey(
      async () => ({
        stdout: JSON.stringify({ ...staged, phase: "active", revision: 5 }),
      }),
      staged,
      options,
    ),
  );
});
