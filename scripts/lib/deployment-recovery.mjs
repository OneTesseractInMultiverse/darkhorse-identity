import { createHash, createPublicKey, randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { link, lstat, open, readdir, unlink } from "node:fs/promises";
import { join } from "node:path";
import { compose, stopped } from "./deployment-operations.mjs";

const utf8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const uuidV4 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

async function readRegular(path, consume) {
  if (typeof path !== "string" || !path.length)
    throw new Error("Supply the required recovery input file path.");
  const file = await open(
    path,
    constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
  );
  try {
    const before = await file.stat({ bigint: true });
    if (!before.isFile() || before.size > Number.MAX_SAFE_INTEGER)
      throw new Error("Recovery inputs must be ordinary files.");
    const result = await consume(file, before);
    const after = await file.stat({ bigint: true });
    if (before.ctimeNs !== after.ctimeNs || before.size !== after.size)
      throw new Error("Recovery input changed during reading.");
    return result;
  } finally {
    await file.close();
  }
}

async function smallFile(path, maximum, control = false) {
  return readRegular(path, async (file, info) => {
    if (info.size === 0n || info.size > maximum)
      throw new Error("Invalid recovery input size.");
    if (control && (info.nlink !== 1n || (info.mode & 0o7777n) !== 0o444n))
      throw new Error("Recovery identity must be an ordinary read-only file.");
    const bytes = Buffer.alloc(Number(info.size));
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await file.read(
        bytes,
        offset,
        bytes.length - offset,
      );
      if (!bytesRead)
        throw new Error("Recovery input truncated during reading.");
      offset += bytesRead;
    }
    return bytes;
  });
}

async function archiveDigest(path) {
  return readRegular(path, async (file, info) => {
    const hash = createHash("sha256");
    let length = 0;
    if (info.size) {
      const stream = file.createReadStream({
        autoClose: false,
        start: 0,
        end: Number(info.size) - 1,
      });
      for await (const chunk of stream) {
        hash.update(chunk);
        length += chunk.length;
      }
    }
    if (length !== Number(info.size))
      throw new Error("Recovery archive truncated during hashing.");
    return hash.digest("hex");
  });
}

async function deploymentIdentity(stack) {
  const directory = join(stack.directory, "recovery");
  const info = await lstat(directory);
  if (!info.isDirectory() || (info.mode & 0o7777) !== 0o755)
    throw new Error("Invalid recovery control directory.");
  const identity = utf8.decode(
    await smallFile(join(directory, "deployment"), 4096, true),
  );
  const [deployment] = identity.split("\n");
  if (
    !uuidV4.test(deployment) ||
    identity !== `${deployment}\n${stack.settings.origin}\n`
  )
    throw new Error("Recovery identity must match the stack issuer.");
  return deployment;
}

async function optionalStat(path) {
  try {
    return await lstat(path);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    return null;
  }
}

// Caller holds the deployment operation lock. An interrupted pre-publication
// write may be resumed only for the same key, in its own private staging file.
async function stageKey(path, bytes) {
  const pending = await optionalStat(path);
  if (
    pending &&
    (!pending.isFile() ||
      pending.nlink !== 1 ||
      pending.uid !== process.getuid() ||
      ![0o600, 0o444].includes(pending.mode & 0o7777) ||
      pending.size > bytes.length)
  )
    throw new Error("Unsafe recovery enrollment staging file.");
  if (!pending || (pending.mode & 0o7777) === 0o600) {
    const file = await open(
      path,
      pending
        ? constants.O_RDWR | constants.O_NOFOLLOW | constants.O_NONBLOCK
        : "wx",
      0o600,
    );
    try {
      if (pending) {
        const info = await file.stat();
        if (
          !info.isFile() ||
          info.ino !== pending.ino ||
          info.dev !== pending.dev ||
          info.nlink !== 1 ||
          info.size > bytes.length ||
          (info.mode & 0o7777) !== 0o600
        )
          throw new Error(
            "Cannot confirm the interrupted recovery enrollment.",
          );
        const prefix = Buffer.alloc(info.size);
        if (
          (await file.read(prefix, 0, prefix.length, 0)).bytesRead !==
            prefix.length ||
          !bytes.subarray(0, prefix.length).equals(prefix)
        )
          throw new Error(
            "Cannot confirm the interrupted recovery enrollment.",
          );
      }
      await file.writeFile(bytes);
      await file.chmod(0o444);
      await file.sync();
    } finally {
      await file.close();
    }
  }
  if (!(await smallFile(path, 32, true)).equals(bytes))
    throw new Error("Staged recovery key differs from supplied public key.");
}

export async function syncDirectory(path) {
  const dir = await open(
    path,
    constants.O_DIRECTORY | constants.O_RDONLY | constants.O_NOFOLLOW,
  );
  try {
    await dir.sync();
  } finally {
    await dir.close();
  }
}

export async function enrollRecovery(stack, publicKeyPath) {
  const deployment = await deploymentIdentity(stack);
  const directory = join(stack.directory, "recovery");
  const names = await readdir(directory);
  const permit = await smallFile(join(directory, "serving"), 4096, true);
  if (
    names.some(
      (entry) => !["deployment", "serving", "administrator"].includes(entry),
    ) ||
    utf8.decode(permit) !== `${deployment}\n${stack.settings.origin}\n`
  )
    throw new Error(
      "Recovery enrollment requires the original serving permission.",
    );
  const pem = utf8.decode(await smallFile(publicKeyPath, 4096));
  if (
    !/^-----BEGIN PUBLIC KEY-----\r?\n(?:[A-Za-z0-9+/=]+\r?\n)+-----END PUBLIC KEY-----(?:\r?\n)?$/.test(
      pem,
    )
  )
    throw new Error("Supply a single Ed25519 PUBLIC KEY PEM.");
  const key = createPublicKey(pem);
  if (key.asymmetricKeyType !== "ed25519")
    throw new Error(
      "An Ed25519 recovery administrator public key is required.",
    );
  const bytes = Buffer.from(key.export({ format: "jwk" }).x, "base64url");
  const fingerprint = createHash("sha256").update(bytes).digest("hex");
  const staging = join(stack.directory, `.administrator-${fingerprint}`);
  const destination = join(directory, "administrator");
  const enrolled = await optionalStat(destination);
  if (!enrolled) {
    await stageKey(staging, bytes);
    await link(staging, destination);
  } else if (
    !enrolled.isFile() ||
    (enrolled.mode & 0o7777) !== 0o444 ||
    !(await smallFile(destination, 32)).equals(bytes)
  ) {
    throw new Error("Cannot replace the enrolled recovery administrator.");
  }
  // Persist the pin before removing its staging link. A retry may reconcile
  // the second link only after checking the supplied pin and both inodes.
  await syncDirectory(directory);
  const pin = await lstat(destination, { bigint: true });
  if (pin.nlink !== 1n) {
    const stage = await lstat(staging, { bigint: true });
    if (
      pin.nlink !== 2n ||
      !stage.isFile() ||
      pin.ino !== stage.ino ||
      pin.dev !== stage.dev
    )
      throw new Error("Unrecognized recovery administrator hard link.");
    await unlink(staging);
  }
  await syncDirectory(stack.directory);
  return enrolled ? "already enrolled" : "enrolled";
}

export async function makeRecoveryRequest(stack, archive) {
  const deployment = await deploymentIdentity(stack);
  return (
    JSON.stringify({
      action: "darkhorse.recovery.begin.v1",
      deployment,
      issuer: stack.settings.origin,
      operation: randomUUID(),
      archive_sha256: await archiveDigest(archive),
    }) + "\n"
  );
}

export async function beginRecovery(
  stack,
  { archive, request, signature },
  options = {},
) {
  await stopped(stack);
  const signedRequest = utf8.decode(await smallFile(request, 4096));
  const detached = await smallFile(signature, 64);
  if (detached.length !== 64)
    throw new Error("Expected a raw 64-byte detached Ed25519 signature.");
  const input =
    JSON.stringify({
      request: signedRequest,
      signature: detached.toString("base64"),
      archive_sha256: await archiveDigest(archive),
    }) + "\n";
  if (Buffer.byteLength(input) > 16384)
    throw new Error("Recovery input exceeds 16 KiB.");
  return compose(
    stack,
    [
      "run",
      "--rm",
      "--no-deps",
      "-T",
      "--user",
      `${process.getuid()}:${process.getgid()}`,
      "recovery",
      "--yes",
      "operator",
      "recovery",
      "begin",
    ],
    { ...options, input },
  );
}
