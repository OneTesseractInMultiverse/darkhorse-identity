import test from "node:test";
import assert from "node:assert/strict";
import {
  readLogoutSnapshot,
  assertQuarantinedLogoutHistory,
} from "../../lib/logout-restore-fixture.mjs";

test("snapshot reader rejects an unassigned database before executing a command", async () => {
  await assert.rejects(
    readLogoutSnapshot(() => assert.fail("must not execute"), {}, "production"),
    /database rejected/,
  );
});

test("snapshot reads use quiet fail-fast psql and reject corrupt output", async () => {
  for (const database of ["darkhorse", "quarantine"]) {
    let calls = 0;
    const compose = async (stack, args, options) => {
      calls++;
      assert.equal(stack.owner, "source-defined-run");
      assert.equal(args[args.indexOf("-d") + 1], database);
      assert.ok(
        args.includes("-X") &&
          args.includes("-qAt") &&
          args.includes("ON_ERROR_STOP=1"),
      );
      assert.equal(options.capture, true);
      return { stdout: "corrupt output", code: 0 };
    };
    await assert.rejects(
      readLogoutSnapshot(compose, { owner: "source-defined-run" }, database),
      /Logout restore JSON/,
    );
    assert.equal(calls, 1);
  }
});

test("immutable-history probe targets only quarantine and propagates SQL failure", async () => {
  let calls = 0;
  const compose = async (_stack, args, options) => {
    calls++;
    assert.equal(args[args.indexOf("-d") + 1], "quarantine");
    assert.equal(options.capture, true);
    throw new Error("source-defined database failure");
  };
  await assert.rejects(
    assertQuarantinedLogoutHistory(compose, {}),
    /source-defined database failure/,
  );
  assert.equal(calls, 1);
});
