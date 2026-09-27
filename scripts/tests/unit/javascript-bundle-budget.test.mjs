import assert from "node:assert/strict";
import test from "node:test";
import { javascriptBundleBudget } from "../../lib/javascript-bundle-budget.mjs";

test("accepts a production JavaScript bundle at its recorded ceiling", () => {
  assert.deepEqual(
    javascriptBundleBudget({
      maximumGzipBytes: 225_533,
      currentGzipBytes: 225_533,
    }),
    {
      maximumGzipBytes: 225_533,
      remainingGzipBytes: 0,
      withinBudget: true,
    },
  );
});

test("rejects a one-byte production bundle regression beyond its recorded ceiling", () => {
  assert.equal(
    javascriptBundleBudget({
      maximumGzipBytes: 225_533,
      currentGzipBytes: 225_534,
    }).withinBudget,
    false,
  );
});

test("rejects incomplete, negative and unsafe bundle measurements", () => {
  for (const input of [
    {},
    {
      maximumGzipBytes: -1,
      currentGzipBytes: 109_153,
    },
    {
      maximumGzipBytes: 225_533,
      currentGzipBytes: Number.MAX_SAFE_INTEGER + 1,
    },
  ]) {
    assert.throws(() => javascriptBundleBudget(input), /measurement/i);
  }
});

test("supports a zero-sized test asset while retaining a positive ceiling", () => {
  assert.deepEqual(
    javascriptBundleBudget({
      maximumGzipBytes: 225_533,
      currentGzipBytes: 0,
    }),
    {
      maximumGzipBytes: 225_533,
      remainingGzipBytes: 225_533,
      withinBudget: true,
    },
  );
});
