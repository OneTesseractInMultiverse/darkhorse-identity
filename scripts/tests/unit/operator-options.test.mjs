import test from "node:test";
import assert from "node:assert/strict";
import {
  listingOptions,
  nonzeroUuid,
  protectedStdin,
  UsageError,
} from "../../lib/operator-options.mjs";
test("shared page options use Unicode character bounds and reject control or ambiguous whitespace", () => {
  assert.deepEqual(listingOptions({}), ["--limit", "25"]);
  assert.deepEqual(
    listingOptions({ search: "--help", status: "active", limit: "1" }),
    ["--limit", "1", "--search=--help", "--status", "active"],
  );
  for (const search of [
    " padded",
    "padded ",
    "x\u0000y",
    "x\u007fy",
    "x\u0085y",
    "x".repeat(101),
  ])
    assert.throws(() => listingOptions({ search }), UsageError);
  assert.doesNotThrow(() => listingOptions({ search: "😀".repeat(100) }));
});
test("UUID selectors exclude zero, abbreviated and option-like values", () => {
  assert.equal(nonzeroUuid("ABCDEF00-1234-5678-9ABC-DEF012345678"), true);
  for (const value of [
    undefined,
    "",
    "--flag",
    "abcd",
    "00000000-0000-0000-0000-000000000000",
    "ABCDEF00123456789ABCDEF012345678",
  ])
    assert.equal(nonzeroUuid(value), false);
  assert.doesNotThrow(() => protectedStdin(false));
  assert.throws(() => protectedStdin(true), UsageError);
});
