import test from "node:test";
import assert from "node:assert/strict";
import { cleanupBoundary } from "../../lib/boundary-cleanup.mjs";
const owner = "a".repeat(32),
  container = "b".repeat(64),
  network = "c".repeat(64);
test("cleanup selects one owner and removes only exact IDs before verifying absence", async () => {
  const calls = [],
    outputs = [container, "", network, "", "", ""];
  const report = await cleanupBoundary(owner, async (file, args, options) => {
    calls.push({ file, args, options });
    return outputs.shift();
  });
  assert.equal(report.status, "verified");
  assert.equal(report.containersRemoved, 1);
  assert.equal(report.networksRemoved, 1);
  assert.deepEqual(calls[1].args, ["rm", "--force", "--volumes", container]);
  assert.deepEqual(calls[3].args, ["network", "rm", network]);
  for (const i of [0, 2, 4, 5])
    assert.ok(
      calls[i].args.includes(`label=org.darkhorse.boundary-run=${owner}`),
    );
  assert.ok(
    calls.every(
      (call) => call.file === "docker" && call.options.timeout <= 20_000,
    ),
  );
});
test("unowned, malformed, failed or remaining resources never produce successful cleanup", async () => {
  for (const value of [undefined, "", "wrong"])
    await assert.rejects(
      cleanupBoundary(value, () => assert.fail("must not invoke Docker")),
    );
  await assert.rejects(cleanupBoundary(owner, async () => "--all"));
  await assert.rejects(
    cleanupBoundary(owner, async () => {
      throw new Error("offline");
    }),
  );
  await assert.rejects(
    cleanupBoundary(owner, async (_file, args) =>
      args[0] === "ps" ? container : "",
    ),
  );
  const report = await cleanupBoundary(owner, async () => "");
  assert.deepEqual(report, {
    status: "verified",
    containersRemoved: 0,
    networksRemoved: 0,
  });
});
