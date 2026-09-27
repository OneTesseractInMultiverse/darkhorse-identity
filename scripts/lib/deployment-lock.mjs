import { randomBytes } from "node:crypto";
import { hostname } from "node:os";
import { lstat, mkdir, open, readFile, unlink } from "node:fs/promises";
import { join, resolve } from "node:path";

const stackNamePattern = /^[a-z][a-z0-9-]{0,31}$/;
const operationPattern = /^[a-z][a-z0-9-]{0,31}$/;

export async function withStackOperation(name, operation, perform) {
  const lock = await acquireStackOperation(name, operation);
  try {
    return await perform();
  } finally {
    await lock.release();
  }
}

export async function acquireStackOperation(name, operation) {
  const path = lockPath(name);
  if (!operationPattern.test(operation))
    throw new Error("Invalid managed Compose operation.");
  await privateLockDirectory();

  const token = randomBytes(32).toString("hex");
  let handle;
  try {
    handle = await open(path, "wx", 0o600);
  } catch (error) {
    if (error.code === "EEXIST")
      throw new Error(
        "A managed Compose operation is already locked or left an interrupted lock. Inspect it with make stack-lock-status; do not remove it until the recorded process and Docker operation are confirmed stopped.",
      );
    throw new Error("Cannot create the private Compose operation lock.");
  }

  let identity;
  try {
    const info = await handle.stat();
    if (!info.isFile() || info.nlink !== 1 || info.mode & 0o077)
      throw new Error("Unsafe Compose lock file.");
    identity = { dev: info.dev, ino: info.ino };
    const record = {
      version: 1,
      operation,
      pid: process.pid,
      host: hostname(),
      startedAt: new Date().toISOString(),
      token,
    };
    await handle.writeFile(`${JSON.stringify(record)}\n`, "utf8");
    await handle.sync();
  } catch {
    await handle.close().catch(() => {});
    await removeOwnedLock(path, token, identity);
    throw new Error("Cannot securely initialize the Compose operation lock.");
  }
  await handle.close();

  let released = false;
  return {
    async release() {
      if (released) return;
      await removeOwnedLock(path, token, identity);
      released = true;
    },
  };
}

export async function inspectStackOperation(name) {
  const path = lockPath(name);
  await privateLockDirectory();
  let info;
  try {
    info = await lstat(path);
  } catch (error) {
    if (error.code === "ENOENT") return { status: "available" };
    throw new Error("Cannot inspect the Compose operation lock.");
  }
  if (
    !info.isFile() ||
    info.nlink !== 1 ||
    info.size > 2048 ||
    info.mode & 0o077
  )
    throw new Error(
      "Compose operation lock is malformed or unsafe. Do not remove it until the selected stack is confirmed stopped.",
    );
  let record;
  try {
    record = JSON.parse(await readFile(path, "utf8"));
  } catch {
    throw new Error(
      "Compose operation lock cannot be read. Treat it as active until the selected stack is confirmed stopped.",
    );
  }
  if (
    !record ||
    record.version !== 1 ||
    !operationPattern.test(record.operation) ||
    !Number.isSafeInteger(record.pid) ||
    record.pid <= 0 ||
    typeof record.host !== "string" ||
    record.host.length > 255 ||
    typeof record.startedAt !== "string" ||
    !Number.isFinite(Date.parse(record.startedAt)) ||
    !/^[a-f0-9]{64}$/.test(record.token ?? "")
  )
    throw new Error(
      "Compose operation lock is malformed. Treat it as active until the selected stack is confirmed stopped.",
    );
  return {
    status: "locked",
    operation: record.operation,
    pid: record.pid,
    host: record.host,
    startedAt: record.startedAt,
  };
}

function lockPath(name) {
  if (!stackNamePattern.test(name ?? ""))
    throw new Error("Invalid stack name.");
  return join(resolve(".local/stacks"), `.operation-${name}.lock`);
}

async function privateLockDirectory() {
  const path = resolve(".local/stacks");
  try {
    await mkdir(path, { recursive: true, mode: 0o700 });
  } catch {
    throw new Error("Cannot create the private Compose stack directory.");
  }
  let info;
  try {
    info = await lstat(path);
  } catch {
    throw new Error("Cannot inspect the private Compose stack directory.");
  }
  if (!info.isDirectory() || info.mode & 0o077)
    throw new Error("Owner-only Compose stack directory required.");
}

async function removeOwnedLock(path, token, expectedIdentity) {
  if (!expectedIdentity) return;
  try {
    const info = await lstat(path);
    if (
      !info.isFile() ||
      info.nlink !== 1 ||
      info.dev !== expectedIdentity.dev ||
      info.ino !== expectedIdentity.ino
    )
      throw new Error("Compose operation lock changed while held.");
    const record = JSON.parse(await readFile(path, "utf8"));
    if (record.token !== token)
      throw new Error("Compose operation lock ownership changed while held.");
    await unlink(path);
  } catch (error) {
    if (error.code === "ENOENT")
      throw new Error("Compose operation lock changed while held.");
    if (
      error.message === "Compose operation lock changed while held." ||
      error.message === "Compose operation lock ownership changed while held."
    )
      throw error;
    throw new Error("Cannot safely release the Compose operation lock.");
  }
}
