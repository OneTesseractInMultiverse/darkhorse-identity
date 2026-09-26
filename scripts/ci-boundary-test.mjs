import { redisImage } from "./lib/boundary-images.mjs";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { boundaryProcess } from "./lib/boundary-process.mjs";
import { boundaryResult, resourceLabels } from "./lib/boundary-ci.mjs";
import { cleanupBoundary } from "./lib/boundary-cleanup.mjs";
import { successful } from "./lib/security-command.mjs";

const owner = randomBytes(16).toString("hex");
const labels = resourceLabels(owner);
const network = `darkhorse-ci-fixture-${owner}`;
const image = redisImage;
const summary =
  "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s";
const run = (script, options = {}) =>
  boundaryProcess(process.execPath, ["-e", script], {
    timeoutMs: 5000,
    ...options,
  });
try {
  assert.equal((await run("process.exit(7)")).code, 7);
  const apparent = await run(
    `console.log(${JSON.stringify(summary)}); process.exit(7)`,
  );
  assert.equal(boundaryResult("postgres", apparent).status, "failed");
  assert.equal(
    (
      await boundaryProcess("/nonexistent/darkhorse-test", [], {
        timeoutMs: 1000,
      })
    ).code,
    null,
  );
  const timeout = await run("setInterval(()=>{}, 1000)", { timeoutMs: 100 });
  assert.equal(timeout.interrupted, true);
  const overflow = await run(
    "process.stdout.write('x'.repeat(8192)); setInterval(()=>{},1000)",
    { maxBytes: 1024 },
  );
  assert.equal(overflow.overflow, true);
  assert.ok(overflow.stdout.length <= 1024);
  await successful("docker", ["network", "create", ...labels, network]);
  await successful("docker", [
    "run",
    "--detach",
    ...labels,
    "--network",
    network,
    "--entrypoint",
    "sleep",
    image,
    "300",
  ]);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 100);
  const interrupted = await run("setInterval(()=>{},1000)", {
    signal: controller.signal,
  });
  clearTimeout(timer);
  assert.equal(interrupted.interrupted, true);
  const cleanup = await cleanupBoundary(owner);
  assert.equal(cleanup.containersRemoved, 1);
  assert.equal(cleanup.networksRemoved, 1);
  assert.equal((await cleanupBoundary(owner)).containersRemoved, 0);
  console.log(
    "Boundary process fixture passed: nonzero exit, missing command, deadline, output bound, interruption and exact-owner Docker cleanup.",
  );
} finally {
  await cleanupBoundary(owner);
}
