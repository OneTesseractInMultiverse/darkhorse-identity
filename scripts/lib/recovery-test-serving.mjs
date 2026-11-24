import assert from "node:assert/strict";
import { chmod, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { run } from "./command.mjs";
import { compose } from "./deployment-operations.mjs";
import { httpsCall } from "./deployment-client.mjs";

const message = "Recovery control denies startup.";
const captured = { capture: true };

async function deniedNativeStart(stack, args) {
  // An invalid DB setting distinguishes recovery denial from runtime startup.
  // timeout runs *inside* the one-shot so a regression cannot leave a server.
  const result = await compose(
    stack,
    [
      "run",
      "--rm",
      "--no-deps",
      "-T",
      "-e",
      "DARKHORSE_DATABASE_URL_FILE=/does-not-exist",
      "--entrypoint",
      "/usr/bin/timeout",
      "api",
      "--kill-after=1s",
      "5s",
      "/usr/local/bin/darkhorse-server",
      ...args,
    ],
    { ...captured, acceptFailure: true },
  );
  assert.equal(result.code, 1, "native process must exit at admission");
  assert.ok(
    result.stdout === "",
    "no runtime output before recovery admission",
  );
  assert.ok(
    result.stderr.includes(message),
    "native recovery denial precedes runtime settings",
  );
}

export async function checkDamagedServingPermit(stack, fixture) {
  // Only this stopped, disposable fixture owns the permit. Retain its inode
  // outside the control directory so this exercise can restore it exactly.
  const permit = join(fixture.control, "serving");
  const saved = join(fixture.directory, "saved-serving");
  await rename(permit, saved);
  try {
    await deniedNativeStart(stack, ["serve-managed"]);
    await writeFile(permit, "truncated\n", { flag: "wx", mode: 0o444 });
    try {
      await chmod(permit, 0o444);
      await deniedNativeStart(stack, ["serve-managed"]);
    } finally {
      await unlink(permit);
    }
  } finally {
    await rename(saved, permit);
  }
}

async function denialsInApiLog(stack) {
  const logs = await compose(
    stack,
    ["logs", "--no-color", "--tail", "100", "api"],
    captured,
  );
  return (logs.stdout + logs.stderr).split(message).length - 1;
}

async function waitForDenial(stack, before) {
  for (let index = 0; index < 30; index++) {
    if ((await denialsInApiLog(stack)) > before) return;
    await delay(200, undefined, { signal: stack.signal });
  }
  assert.fail("restarted packaged API did not report a new recovery denial");
}

export async function checkQuarantinedStarts(stack) {
  for (const args of [["serve-managed"], ["serve"], []])
    await deniedNativeStart(stack, args);
}

export async function checkQuarantinedRestart(stack) {
  const ca = await readFile(join(stack.directory, "secrets/ca.pem"));
  try {
    const up = await run(
      "make",
      ["--no-print-directory", "--silent", "stack-up"],
      {
        ...captured,
        acceptFailure: true,
        signal: stack.signal,
        env: { ...stack.env, STACK: stack.settings.name },
      },
    );
    assert.notEqual(
      up.code,
      0,
      "managed stack-up must refuse quarantined service",
    );
    assert.ok(
      (await denialsInApiLog(stack)) > 0,
      "packaged API must report the recovery gate",
    );
    const live = await httpsCall(
      stack.settings.origin,
      ca,
      "/health/live",
    ).catch(() => null);
    assert.notEqual(live?.status, 200, "quarantined API must remain offline");
    await compose(stack, ["stop", "edge", "api"], captured);
    const before = await denialsInApiLog(stack);
    await compose(stack, ["restart", "api"], captured);
    await waitForDenial(stack, before);
  } finally {
    await compose(stack, ["stop", "edge", "api"], {
      ...captured,
      signal: undefined,
    });
  }
}
