import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, randomUUID, sign } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { run } from "./command.mjs";
import { compose } from "./deployment-operations.mjs";
import {
  checkDamagedServingPermit,
  checkQuarantinedStarts,
  checkQuarantinedRestart,
} from "./recovery-test-serving.mjs";

const captured = { capture: true, acceptFailure: true };
const uuid =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const encode = (value) => JSON.stringify(value) + "\n";

function make(stack, target, paths) {
  return run("make", ["--no-print-directory", "--silent", target], {
    env: { ...stack.env, STACK: stack.settings.name, ...paths },
    signal: stack.signal,
    ...captured,
  });
}

export async function enrollRecoveryFixture(stack) {
  const directory = join(stack.directory, "test-recovery-client");
  await mkdir(directory, { mode: 0o700 });
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const pem = publicKey.export({ type: "spki", format: "pem" });
  const publicPath = join(directory, "administrator.pem");
  await writeFile(publicPath, pem, { mode: 0o600, flag: "wx" });
  const enroll = (path) =>
    make(stack, "stack-recovery-enroll", { RECOVERY_PUBLIC_KEY: path });
  assert.equal(
    (await enroll(publicPath)).code,
    0,
    "enroll independent administrator",
  );
  const other = generateKeyPairSync("ed25519");
  const otherPath = join(directory, "other.pem");
  await writeFile(
    otherPath,
    other.publicKey.export({ type: "spki", format: "pem" }),
    {
      mode: 0o600,
      flag: "wx",
    },
  );
  assert.notEqual(
    (await enroll(otherPath)).code,
    0,
    "enrollment cannot replace trust",
  );
  const rawKey = Buffer.from(
    publicKey.export({ format: "jwk" }).x,
    "base64url",
  );
  const control = join(stack.directory, "recovery");
  assert.ok(
    (await readFile(join(control, "administrator"))).equals(rawKey),
    "enrolled key is immutable",
  );
  return {
    directory,
    control,
    publicPath,
    privateKey,
    other: other.privateKey,
    rawKey,
  };
}

async function requestFor(stack, path, expectedDigest) {
  const result = await make(stack, "stack-recovery-request", {
    RECOVERY_ARCHIVE: path,
  });
  assert.equal(result.code, 0, "generate unsigned recovery request");
  const value = JSON.parse(result.stdout);
  assert.match(value.deployment, uuid);
  assert.match(value.operation, uuid);
  assert.equal(value.action, "darkhorse.recovery.begin.v1");
  assert.equal(value.issuer, stack.settings.origin);
  assert.equal(value.archive_sha256, expectedDigest);
  assert.equal(
    result.stdout,
    encode(value),
    "exact signable request including its final LF",
  );
  return { text: result.stdout, value };
}

export async function checkServingRecoveryLock(stack, fixture) {
  // The disposable public file supplies archive bytes for this overlap only.
  // The end-to-end admission below uses the verified PostgreSQL archive.
  const sha = digest(await readFile(fixture.publicPath));
  const { text } = await requestFor(stack, fixture.publicPath, sha);
  const result = await nativeBegin(stack, fixture, text, sha);
  assert.equal(result.code, 1, "native admission while serving must fail");
  assert.ok(
    result.stderr.includes("Recovery request denied."),
    "live service holds the gate",
  );
  assert.deepEqual((await readdir(fixture.control)).sort(), [
    "administrator",
    "deployment",
    "serving",
  ]);
}

function nativeBegin(stack, fixture, text, sha, options = [], command = []) {
  return compose(
    stack,
    [
      "run",
      "--rm",
      "--no-deps",
      "-T",
      "--user",
      `${process.getuid()}:${process.getgid()}`,
      ...options,
      "--entrypoint",
      "/usr/bin/timeout",
      "recovery",
      "--kill-after=1s",
      "30s",
      ...command,
      "/usr/local/bin/darkhorse-server",
      "--yes",
      "operator",
      "recovery",
      "begin",
    ],
    {
      ...captured,
      input: JSON.stringify({
        request: text,
        signature: sign(null, Buffer.from(text), fixture.privateKey).toString(
          "base64",
        ),
        archive_sha256: sha,
      }),
    },
  );
}

async function interruptAfterReceipt(stack, fixture, text, sha, receipt) {
  const owner = `darkhorse.test.recovery-interruption=${randomUUID()}`;
  // Withhold the real command's output. The bounded sleep keeps the container
  // available to kill if Rust has already exited by the time the host observes
  // the atomic rename. No success response can reach the caller in either case.
  let ended = false;
  const result = nativeBegin(
    stack,
    fixture,
    text,
    sha,
    ["--label", owner],
    ["sh", "-ec", '"$@" >/dev/null; exec sleep 25', "sh"],
  )
    .catch(() => ({ code: null }))
    .finally(() => {
      ended = true;
    });
  let killed, lost;
  try {
    let observed = false;
    for (let attempts = 0; attempts < 125 && !ended; attempts++) {
      if ((await readdir(fixture.control)).includes(receipt)) {
        observed = true;
        break;
      }
      await delay(100, undefined, { signal: stack.signal });
    }
    assert.ok(observed, "recovery receipt before interruption");
  } finally {
    const owned = await run(
      "docker",
      [
        "ps",
        "--quiet",
        "--no-trunc",
        "--filter",
        `label=${owner}`,
        "--filter",
        `label=com.docker.compose.project=${stack.settings.project}`,
      ],
      { capture: true },
    );
    const id = owned.stdout.trim();
    if (/^[0-9a-f]{64}$/.test(id))
      killed = await run("docker", ["kill", id], captured);
    lost = await result;
  }
  assert.equal(
    killed?.code,
    0,
    "fixture terminated the owned recovery container",
  );
  assert.equal(
    lost.code,
    137,
    "recovery container terminated before acknowledgement",
  );
  assert.ok(
    lost.stdout === "",
    "interrupted admission emitted no success response",
  );
}

async function checkArchiveIsolation(stack, fixture, archive, bytes) {
  assert.deepEqual((await readdir(archive)).sort(), [
    "RECOVERY.txt",
    "database.dump",
    "pki",
    "secrets",
    "settings.json",
  ]);
  const [deployment] = (
    await readFile(join(fixture.control, "deployment"), "utf8")
  ).split("\n");
  const seed = Buffer.from(
    fixture.privateKey.export({ format: "jwk" }).d,
    "base64url",
  );
  const patterns = [
    Buffer.from(deployment),
    await readFile(fixture.publicPath),
    Buffer.from(fixture.privateKey.export({ type: "pkcs8", format: "pem" })),
  ];
  for (const key of [seed, fixture.rawKey]) {
    patterns.push(
      key,
      ...["hex", "base64", "base64url"].map((encoding) =>
        Buffer.from(key.toString(encoding)),
      ),
    );
  }
  const check = (content) =>
    assert.ok(
      !patterns.some((pattern) => content.includes(pattern)),
      "backup must exclude independent recovery identity and keys",
    );
  for (const entry of await readdir(archive, {
    recursive: true,
    withFileTypes: true,
  })) {
    if (entry.isDirectory()) continue;
    assert.ok(entry.isFile(), "backup contains only regular files");
    if (entry.name !== "database.dump")
      check(await readFile(join(entry.parentPath, entry.name)));
  }
  const decoded = await compose(
    stack,
    ["exec", "-T", "postgres", "pg_restore", "--file=-"],
    {
      capture: true,
      input: bytes,
    },
  );
  check(Buffer.from(decoded.stdout));
}

export async function admitRecoveryFixture(stack, fixture, archive, bytes) {
  await checkArchiveIsolation(stack, fixture, archive, bytes);
  const archivePath = join(archive, "database.dump");
  const sha = digest(bytes);
  const { text, value } = await requestFor(stack, archivePath, sha);
  const paths = {
    RECOVERY_ARCHIVE: archivePath,
    RECOVERY_REQUEST: join(fixture.directory, "request.json"),
    RECOVERY_SIGNATURE: join(fixture.directory, "request.sig"),
  };
  async function prepare(text, key = fixture.privateKey, signed = text) {
    await writeFile(paths.RECOVERY_REQUEST, text, { mode: 0o600 });
    await writeFile(
      paths.RECOVERY_SIGNATURE,
      sign(null, Buffer.from(signed), key),
      { mode: 0o600 },
    );
  }
  const begin = (overrides) =>
    make(stack, "stack-recovery-begin", { ...paths, ...overrides });
  const servingEntries = ["administrator", "deployment", "serving"];
  async function denied(label, overrides) {
    const result = await begin(overrides);
    assert.notEqual(result.code, 0, label);
    assert.deepEqual(
      (await readdir(fixture.control)).sort(),
      servingEntries,
      label,
    );
  }
  await prepare(text, fixture.other);
  await denied("wrong signer");
  await prepare(text);
  await denied("different archive bytes", {
    RECOVERY_ARCHIVE: fixture.publicPath,
  });
  for (const [field, changed] of [
    ["issuer", "https://other.localhost"],
    ["deployment", randomUUID()],
    ["action", "darkhorse.recovery.release.v1"],
  ]) {
    await prepare(encode({ ...value, [field]: changed }));
    await denied(`signed request with wrong ${field}`);
  }
  await prepare(
    encode({ ...value, operation: randomUUID() }),
    fixture.privateKey,
    text,
  );
  await denied("operation changed after signing");
  await prepare(text.trimEnd(), fixture.privateKey, text);
  await denied("final LF removed after signing");

  await checkDamagedServingPermit(stack, fixture);
  await prepare(text);
  const receipt = `quarantine-${value.operation}-${sha}-${digest(fixture.rawKey)}`;
  await interruptAfterReceipt(stack, fixture, text, sha, receipt);
  const entries = ["administrator", "deployment", receipt].sort();
  assert.deepEqual((await readdir(fixture.control)).sort(), entries);
  assert.ok(
    (await readFile(join(fixture.control, receipt))).equals(
      await readFile(join(fixture.control, "deployment")),
    ),
    "durable receipt retains the deployment and issuer binding",
  );
  await checkQuarantinedStarts(stack);
  assert.equal((await begin()).code, 0, "exact retry from a fresh process");
  await prepare(encode({ ...value, operation: randomUUID() }));
  assert.notEqual(
    (await begin()).code,
    0,
    "conflicting valid request after admission",
  );
  const otherDigest = digest(await readFile(fixture.publicPath));
  await prepare(encode({ ...value, archive_sha256: otherDigest }));
  assert.notEqual(
    (await begin({ RECOVERY_ARCHIVE: fixture.publicPath })).code,
    0,
    "same operation cannot switch archives",
  );
  assert.deepEqual((await readdir(fixture.control)).sort(), entries);
  await checkQuarantinedRestart(stack);
}
