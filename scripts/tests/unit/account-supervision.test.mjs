import test from "node:test";
import assert from "node:assert/strict";
import { superviseAccount } from "../../lib/account-supervision.mjs";
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve };
};
test("completed transport preserves status and does not stop or retry it", async () => {
  let starts = 0,
    stops = 0;
  const result = await superviseAccount(
    {},
    () => {
      starts++;
      return {
        done: Promise.resolve({ code: 74, signal: null }),
        stop: async () => stops++,
      };
    },
    new AbortController().signal,
    new Promise(() => {}),
  );
  assert.equal(result.code, 74);
  assert.equal(result.uncertain, false);
  assert.equal(starts, 1);
  assert.equal(stops, 0);
});
test("timeout and interruption stop only the owned child and do not claim rollback", async () => {
  for (const cause of ["timeout", "signal"]) {
    const timeout = deferred(),
      abort = new AbortController();
    let stops = 0,
      starts = 0;
    const result = superviseAccount(
      {},
      () => {
        starts++;
        return { done: new Promise(() => {}), stop: async () => stops++ };
      },
      abort.signal,
      timeout.promise,
    );
    if (cause === "timeout") timeout.resolve();
    else abort.abort();
    assert.deepEqual(await result, {
      code: cause === "timeout" ? 124 : 130,
      uncertain: true,
    });
    assert.equal(stops, 1);
    assert.equal(starts, 1);
  }
});
test("preexisting interruption launches nothing; startup errors propagate without retry", async () => {
  const abort = new AbortController();
  abort.abort();
  assert.deepEqual(
    await superviseAccount(
      {},
      () => {
        throw new Error("must not run");
      },
      abort.signal,
      new Promise(() => {}),
    ),
    { code: 130, uncertain: true },
  );
  let starts = 0;
  await assert.rejects(
    superviseAccount(
      {},
      () => {
        starts++;
        throw new Error("unavailable");
      },
      new AbortController().signal,
      new Promise(() => {}),
    ),
    /unavailable/,
  );
  assert.equal(starts, 1);
});
test("lost child and asynchronous startup failures stop the attachment once", async () => {
  let stops = 0;
  const signal = new AbortController().signal;
  const start = (done) => () => ({
    done,
    stop: async () => {
      stops++;
    },
  });
  assert.deepEqual(
    await superviseAccount(
      {},
      start(Promise.resolve({ code: null, signal: "SIGKILL" })),
      signal,
      new Promise(() => {}),
    ),
    { code: 1, uncertain: true },
  );
  await assert.rejects(
    superviseAccount(
      {},
      start(Promise.reject(new Error("unavailable"))),
      signal,
      new Promise(() => {}),
    ),
    /unavailable/,
  );
  assert.equal(stops, 2);
});
test("failed cleanup is surfaced once without repeating process-group termination", async () => {
  let stops = 0;
  await assert.rejects(
    superviseAccount(
      {},
      () => ({
        done: new Promise(() => {}),
        stop: async () => {
          stops++;
          throw new Error("cannot stop attachment");
        },
      }),
      new AbortController().signal,
      Promise.resolve(),
    ),
    /cannot stop attachment/,
  );
  assert.equal(stops, 1);
});
