import { test } from "node:test";
import assert from "node:assert/strict";
import { supervise } from "../../lib/supervisor.mjs";

test("stops only started children when a later startup fails", async () => {
  const stopped = [];
  const start = (spec) => {
    if (spec === "broken") throw new Error("missing tool");
    return {
      done: new Promise(() => {}),
      stop: async () => {
        stopped.push(spec);
      },
    };
  };
  await assert.rejects(
    supervise(["api", "broken"], start, new AbortController().signal),
    /missing tool/,
  );
  assert.deepEqual(stopped, ["api"]);
});

test("propagates unexpected child exit and cleans up peers", async () => {
  const stopped = [];
  const start = (spec) => ({
    done: spec === "web" ? Promise.resolve() : new Promise(() => {}),
    stop: async () => {
      stopped.push(spec);
    },
  });
  await assert.rejects(
    supervise(["api", "web"], start, new AbortController().signal),
    /exited/,
  );
  assert.deepEqual(stopped.sort(), ["api", "web"]);
});

test("interruption cleans up all owned children", async () => {
  const stopped = [];
  const abort = new AbortController();
  const start = (spec) => ({
    done: new Promise(() => {}),
    stop: async () => {
      stopped.push(spec);
    },
  });
  const running = supervise(["api", "web"], start, abort.signal);
  abort.abort();
  await running;
  assert.deepEqual(stopped.sort(), ["api", "web"]);
});
