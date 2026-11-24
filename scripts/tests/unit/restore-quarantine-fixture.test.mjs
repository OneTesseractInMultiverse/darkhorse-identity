import test from "node:test";
import assert from "node:assert/strict";
import { assertQuarantineConnectionsDenied } from "../../lib/restore-quarantine-fixture.mjs";

const expected = {
  code: 2,
  stderr:
    'psql: error: connection to server on socket "/fixture/socket" failed: FATAL:  permission denied for database "quarantine"\nDETAIL:  User does not have CONNECT privilege.\n',
};

test("quarantine checks every application login against the restore database", async () => {
  const roles = [];
  const stack = {};
  await assertQuarantineConnectionsDenied(async (selected, args, options) => {
    assert.equal(selected, stack);
    assert.equal(args[args.indexOf("-d") + 1], "quarantine");
    assert.equal(args.at(-1), "SELECT 1");
    assert.equal(options.acceptFailure, true);
    assert.equal(options.capture, true);
    roles.push(args[args.indexOf("-U") + 1]);
    return expected;
  }, stack);
  assert.deepEqual(roles.sort(), [
    "darkhorse_operator",
    "darkhorse_owner",
    "darkhorse_runtime",
  ]);
});

test("only a database CONNECT denial qualifies for each role", async () => {
  for (const failed of [
    { code: 0, stderr: "" },
    { code: 2, stderr: "password authentication failed" },
    { code: 2, stderr: 'FATAL:  permission denied for database "darkhorse"' },
    { code: 1, stderr: expected.stderr },
  ]) {
    for (const role of [
      "darkhorse_owner",
      "darkhorse_operator",
      "darkhorse_runtime",
    ]) {
      const compose = async (_stack, args) =>
        args.includes(role) ? failed : expected;
      await assert.rejects(
        assertQuarantineConnectionsDenied(compose, {}),
        /quarantine CONNECT denial/i,
      );
    }
  }
});
