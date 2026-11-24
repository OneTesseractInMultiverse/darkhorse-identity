import assert from "node:assert/strict";
import {
  chmod,
  copyFile,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { run } from "../../lib/command.mjs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  loadStack,
  setupStack,
  stackDirectory,
} from "../../lib/deployment-state.mjs";

const origin = "https://identity.localhost:9443";
const image = "sha256:" + "a".repeat(64);
const uuid =
  "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";

async function workspace(t) {
  const old = process.cwd();
  const mask = process.umask();
  const directory = await mkdtemp(join(tmpdir(), "darkhorse-recovery-test-"));
  process.chdir(directory);
  t.after(async () => {
    process.chdir(old);
    process.umask(mask);
    await rm(directory, { recursive: true });
  });
}

function commandWithResources({ labelledVolume, namedVolume, container } = {}) {
  return async (command, args) => {
    if (command === "openssl") {
      for (const option of ["-out", "-keyout"]) {
        if (args.includes(option))
          await writeFile(args[args.indexOf(option) + 1], "offline test");
      }
      return { stdout: "" };
    }
    assert.equal(command, "docker");
    if (args[0] === "image") return { stdout: image + "\n" };
    if (
      args.includes("label=com.docker.compose.project=darkhorse-stack-trial")
    ) {
      return {
        stdout: (args[0] === "volume" ? labelledVolume : container) ?? "",
      };
    }
    if (args[0] === "volume" && args[1] === "ls")
      return { stdout: namedVolume ?? "" };
    assert.fail("Unexpected Docker request");
  };
}

test("fresh setup creates an independent deployment and serving permission exactly once", async (t) => {
  await workspace(t);
  const command = commandWithResources();
  const stack = await setupStack("trial", origin, image, command);
  const control = join(stack.directory, "recovery");
  assert.equal((await lstat(control)).mode & 0o777, 0o755);
  const deployment = await readFile(join(control, "deployment"), "utf8");
  assert.match(
    deployment,
    new RegExp("^" + uuid + "\\nhttps://identity\\.localhost:9443\\n$"),
  );
  assert.equal(await readFile(join(control, "serving"), "utf8"), deployment);
  for (const entry of ["deployment", "serving"]) {
    const stat = await lstat(join(control, entry));
    assert.ok(stat.isFile());
    assert.equal(stat.nlink, 1);
    assert.equal(stat.mode & 0o777, 0o444);
  }
  await assert.rejects(lstat(join(control, "administrator")), {
    code: "ENOENT",
  });
  await unlink(join(control, "serving"));
  await setupStack("trial", origin, image, command);
  assert.equal(await readFile(join(control, "deployment"), "utf8"), deployment);
  await assert.rejects(lstat(join(control, "serving")), { code: "ENOENT" });
  await rm(control, { recursive: true });
  await setupStack("trial", origin, image, command);
  assert.equal((await loadStack("trial")).settings.origin, origin);
  await assert.rejects(lstat(control), { code: "ENOENT" });
});

test("setup cannot grant serving permission for pre-existing project data or containers", async (t) => {
  await workspace(t);
  for (const resources of [
    { labelledVolume: "orphan\n" },
    { namedVolume: "darkhorse-stack-trial_postgres-data\n" },
    { namedVolume: "darkhorse-stack-trial_limiter-data\n" },
    { container: "afdb00\n" },
  ]) {
    await assert.rejects(
      setupStack("trial", origin, image, commandWithResources(resources)),
      /existing Docker resources/,
    );
    await assert.rejects(lstat(stackDirectory("trial")), { code: "ENOENT" });
  }
});

async function recoveryMake(t) {
  await workspace(t);
  const repo = join(import.meta.dirname, "../../..");
  await mkdir("scripts");
  await copyFile(
    join(repo, "scripts/deployment.mjs"),
    "scripts/deployment.mjs",
  );
  await symlink(join(repo, "scripts/lib"), "scripts/lib");
  await copyFile(join(repo, "Makefile"), "Makefile");
  const stack = await setupStack(
    "trial",
    origin,
    image,
    commandWithResources(),
  );
  const make = (target, env) =>
    run("make", ["--no-print-directory", "stack-recovery-" + target], {
      capture: true,
      acceptFailure: true,
      env: { ...process.env, MAKEFLAGS: "", STACK: "trial", ...env },
    });
  return { stack, make };
}

test("recovery enrollment accepts an independent Ed25519 public PEM once and preserves its exact key", async (t) => {
  const { stack, make } = await recoveryMake(t);
  const destination = join(stack.directory, "recovery", "administrator");
  const key = generateKeyPairSync("ed25519");
  const pem = key.publicKey.export({ type: "spki", format: "pem" });
  const path = join(process.cwd(), "trust $offline.pem");
  const enroll = () => make("enroll", { RECOVERY_PUBLIC_KEY: path });
  for (const invalid of [
    key.privateKey.export({ type: "pkcs8", format: "pem" }),
    generateKeyPairSync("x25519").publicKey.export({
      type: "spki",
      format: "pem",
    }),
    pem + pem,
  ]) {
    await writeFile(path, invalid);
    assert.notEqual((await enroll()).code, 0);
    await assert.rejects(lstat(destination), { code: "ENOENT" });
  }
  await writeFile(path, pem);
  const result = await enroll();
  assert.equal(result.code, 0, result.stderr);
  const expected = Buffer.from(
    key.publicKey.export({ format: "jwk" }).x,
    "base64url",
  );
  assert.deepEqual(await readFile(destination), expected);
  assert.equal((await lstat(destination)).mode & 0o777, 0o444);
  assert.equal((await enroll()).code, 0);
  const staging = join(
    stack.directory,
    ".administrator-" + createHash("sha256").update(expected).digest("hex"),
  );
  await link(destination, staging);
  const confirmed = await enroll();
  assert.equal(confirmed.code, 0, confirmed.stderr);
  assert.match(confirmed.stdout, /already enrolled/i);
  assert.equal((await lstat(destination)).nlink, 1);
  await assert.rejects(lstat(staging), { code: "ENOENT" });
  // A separate, unrecognized hard link is never removed during a retry.
  const external = join(stack.directory, "custodian-file");
  await link(destination, external);
  assert.notEqual((await enroll()).code, 0);
  assert.deepEqual(await readFile(external), expected);
  await unlink(external);
  await writeFile(
    path,
    generateKeyPairSync("ed25519").publicKey.export({
      type: "spki",
      format: "pem",
    }),
  );
  assert.notEqual((await enroll()).code, 0);
  assert.deepEqual(await readFile(destination), expected);
});

test("enrollment requires the original matching serving permission and clean control directory", async (t) => {
  const { stack, make } = await recoveryMake(t);
  await writeFile(
    "public.pem",
    generateKeyPairSync("ed25519").publicKey.export({
      format: "pem",
      type: "spki",
    }),
  );
  const enroll = () => make("enroll", { RECOVERY_PUBLIC_KEY: "public.pem" });
  const control = join(stack.directory, "recovery");
  const extra = join(control, "quarantine-interrupted");
  const serving = join(control, "serving");
  await writeFile(extra, "unexpected");
  assert.notEqual((await enroll()).code, 0);
  await assert.rejects(lstat(join(control, "administrator")), {
    code: "ENOENT",
  });
  await unlink(extra);
  await unlink(serving);
  assert.notEqual((await enroll()).code, 0);
  await writeFile(serving, "different issuer/deployment");
  await chmod(serving, 0o444);
  assert.notEqual((await enroll()).code, 0);
  await assert.rejects(lstat(join(control, "administrator")), {
    code: "ENOENT",
  });
});

test("recovery request binds fresh operations to the deployment, issuer and exact archive bytes", async (t) => {
  const { stack, make } = await recoveryMake(t);
  const archive = join(process.cwd(), "database $archive.dump");
  await writeFile(archive, "abc");
  const request = () => make("request", { RECOVERY_ARCHIVE: archive });
  const result = await request();
  assert.equal(result.code, 0, result.stderr);
  const record = JSON.parse(result.stdout);
  const deployment = (
    await readFile(join(stack.directory, "recovery/deployment"), "utf8")
  ).split("\n")[0];
  assert.match(record.operation, new RegExp("^" + uuid + "$"));
  const expected = {
    action: "darkhorse.recovery.begin.v1",
    deployment,
    issuer: origin,
    operation: record.operation,
    archive_sha256:
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  };
  assert.equal(result.stdout, JSON.stringify(expected) + "\n");
  assert.notEqual(
    JSON.parse((await request()).stdout).operation,
    record.operation,
  );
  await unlink(archive);
  await symlink(join(stack.directory, "settings.json"), archive);
  assert.notEqual((await request()).code, 0);
});

test("recovery begin transmits exact signed bytes and independently hashes the selected archive after the stopped check", async (t) => {
  const { make } = await recoveryMake(t);
  const key = generateKeyPairSync("ed25519");
  const archive = join(process.cwd(), "database.dump");
  const request = join(process.cwd(), "signed.request");
  const signature = join(process.cwd(), "detached.signature");
  await writeFile(archive, "abc");
  const signed = await make("request", { RECOVERY_ARCHIVE: archive });
  assert.equal(signed.code, 0, signed.stderr);
  await writeFile(request, signed.stdout);
  const sig = sign(null, Buffer.from(signed.stdout), key.privateKey);
  await writeFile(signature, sig);
  // Stand in for Docker only: the lock, stopped check, host I/O, hashing and
  // compose command-line/stdin construction all use the production entrypoints.
  await writeFile(
    "docker",
    `#!/usr/bin/env node
const args = process.argv.slice(2);
if (args.includes("ps")) {
  console.log(process.env.RECOVERY_TEST_SERVICE || "postgres");
} else if (args.includes("run")) {
  let input = "";
  process.stdin.on("data", (chunk) => input += chunk);
  process.stdin.on("end", () => console.log(JSON.stringify({args:args.slice(args.indexOf("run")), input:JSON.parse(input)})));
} else process.exitCode = 2;
`,
    { mode: 0o755 },
  );
  const begin = (service) =>
    make("begin", {
      RECOVERY_ARCHIVE: archive,
      RECOVERY_REQUEST: request,
      RECOVERY_SIGNATURE: signature,
      RECOVERY_TEST_SERVICE: service,
      PATH: process.cwd() + ":" + process.env.PATH,
    });
  const busy = await begin("api");
  assert.notEqual(busy.code, 0);
  assert.equal(busy.stdout, "");
  await writeFile(archive, "abc\n");
  const ready = await begin("postgres");
  assert.equal(ready.code, 0, ready.stderr);
  const run = JSON.parse(ready.stdout);
  assert.deepEqual(run.args, [
    "run",
    "--rm",
    "--no-deps",
    "-T",
    "--user",
    process.getuid() + ":" + process.getgid(),
    "recovery",
    "--yes",
    "operator",
    "recovery",
    "begin",
  ]);
  assert.deepEqual(run.input, {
    request: signed.stdout,
    signature: sig.toString("base64"),
    archive_sha256:
      "edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb",
  });
  for (const [file, bytes] of [
    [signature, Buffer.alloc(65)],
    [request, Buffer.alloc(16384)],
  ]) {
    await writeFile(file, bytes);
    assert.notEqual((await begin("postgres")).code, 0);
    await writeFile(file, file === signature ? sig : signed.stdout);
  }
});
