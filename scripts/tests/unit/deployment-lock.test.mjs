import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
  mkdtemp,
  chmod,
  lstat,
  readFile,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { hostname, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  acquireStackOperation,
  inspectStackOperation,
  withStackOperation,
} from "../../lib/deployment-lock.mjs";

test("Compose operation locks serialize managed writers and fail closed after interruption", async () => {
  const previous = process.cwd();
  const directory = await mkdtemp(join(tmpdir(), "darkhorse-stack-lock-"));
  process.chdir(directory);
  try {
    assert.deepEqual(await inspectStackOperation("trial"), {
      status: "available",
    });
    const held = await acquireStackOperation("trial", "backup");
    const lockPath = join(directory, ".local/stacks/.operation-trial.lock");
    const info = await lstat(lockPath);
    assert.ok(info.isFile());
    assert.equal(info.mode & 0o777, 0o600);
    assert.deepEqual(await inspectStackOperation("trial"), {
      status: "locked",
      operation: "backup",
      pid: process.pid,
      host: hostname(),
      startedAt: (await readFile(lockPath, "utf8")).match(
        /"startedAt":"([^"]+)"/,
      )[1],
    });
    await assert.rejects(
      acquireStackOperation("trial", "up"),
      /already locked/,
    );
    await held.release();
    await held.release();
    assert.deepEqual(await inspectStackOperation("trial"), {
      status: "available",
    });

    await assert.rejects(
      withStackOperation("trial", "migrate", async () => {
        throw new Error("synthetic migration failure");
      }),
      /synthetic migration failure/,
    );
    assert.deepEqual(await inspectStackOperation("trial"), {
      status: "available",
    });

    const child = spawn(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `const {acquireStackOperation}=await import(${JSON.stringify(pathToFileURL(resolve(import.meta.dirname, "../../lib/deployment-lock.mjs")).href)}); await acquireStackOperation("trial","up"); console.log("ready"); setInterval(()=>{},1000);`,
      ],
      { cwd: directory, stdio: ["ignore", "pipe", "pipe"] },
    );
    let childOutput = "";
    child.stdout.on("data", (data) => {
      childOutput += data;
    });
    const closed = new Promise((resolveClosed, rejectClosed) => {
      child.once("close", resolveClosed);
      child.once("error", rejectClosed);
    });
    try {
      await new Promise((resolveReady, rejectReady) => {
        const timeout = setTimeout(
          () => rejectReady(new Error("Lock subprocess did not start.")),
          5000,
        );
        child.stdout.on("data", (data) => {
          if (data.toString().includes("ready")) {
            clearTimeout(timeout);
            resolveReady();
          }
        });
        child.once("error", rejectReady);
      });
      assert.ok(childOutput.includes("ready"));
      await assert.rejects(
        acquireStackOperation("trial", "down"),
        /already locked/,
      );
    } finally {
      if (child.exitCode === null && child.signalCode === null)
        child.kill("SIGTERM");
      await closed;
    }
    assert.equal((await inspectStackOperation("trial")).operation, "up");
    await unlink(lockPath);
    assert.deepEqual(await inspectStackOperation("trial"), {
      status: "available",
    });

    const removed = await acquireStackOperation("trial", "status");
    await unlink(lockPath);
    await assert.rejects(removed.release(), /lock changed while held/);

    const replaced = await acquireStackOperation("trial", "backup");
    await unlink(lockPath);
    await writeFile(
      lockPath,
      `${JSON.stringify({
        version: 1,
        operation: "down",
        pid: process.pid,
        host: "fixture-host",
        startedAt: new Date().toISOString(),
        token: "b".repeat(64),
      })}\n`,
      { mode: 0o600 },
    );
    await assert.rejects(replaced.release(), /lock (?:ownership )?changed while held/);
    assert.equal((await inspectStackOperation("trial")).operation, "down");
    await unlink(lockPath);

    await assert.rejects(
      acquireStackOperation("../escape", "up"),
      /Invalid stack name/,
    );
    await assert.rejects(
      acquireStackOperation("trial", "invalid operation"),
      /Invalid managed Compose operation/,
    );
    await chmod(join(directory, ".local/stacks"), 0o755);
    await assert.rejects(acquireStackOperation("trial", "up"), /Owner-only/);
  } finally {
    process.chdir(previous);
    await rm(directory, { recursive: true, force: true });
  }
});
