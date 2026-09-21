import { readFile, mkdir, open, cp, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { operatorArgs } from "./deployment-plan.mjs";
import { run } from "./command.mjs";
import { httpsCall } from "./deployment-client.mjs";
export function compose(stack, args, options = {}) {
  return run("docker", [...stack.args, ...args], {
    env: stack.env,
    signal: stack.signal,
    ...options,
  });
}
export function operator(stack, command, args = [], options = {}) {
  const tty =
    command === "bootstrap" &&
    !args.length &&
    process.stdin.isTTY &&
    options.input === undefined;
  return compose(
    stack,
    [
      "run",
      "--rm",
      "--no-deps",
      ...(tty ? [] : ["-T"]),
      "operator",
      ...operatorArgs(command, args),
    ],
    options,
  );
}
export async function stopped(stack) {
  const running = (
    await compose(
      stack,
      [
        "ps",
        "--all",
        "--status",
        "running",
        "--status",
        "restarting",
        "--status",
        "paused",
        "--status",
        "created",
        "--services",
      ],
      {
        capture: true,
      },
    )
  ).stdout.split("\n");
  if (running.some((v) => ["api", "edge", "operator"].includes(v)))
    throw new Error(
      "Stop the application and finish operator jobs before migrations or backup.",
    );
}
export async function migrate(stack) {
  await stopped(stack);
  await operator(stack, "migrate");
  await compose(
    stack,
    [
      "exec",
      "-T",
      "postgres",
      "psql",
      "-U",
      "postgres",
      "-d",
      "darkhorse",
      "-v",
      "ON_ERROR_STOP=1",
    ],
    {
      input: await readFile(resolve("deploy/grant-runtime.sql"), "utf8"),
      capture: true,
    },
  );
  console.log("Schema migrated and reviewed runtime grants applied.");
}
export async function check(stack) {
  const ca = await readFile(join(stack.directory, "secrets/ca.pem"));
  const health = await httpsCall(stack.settings.origin, ca, "/health/live");
  const discovery = await httpsCall(
    stack.settings.origin,
    ca,
    "/.well-known/openid-configuration",
  );
  if (
    health.status !== 200 ||
    discovery.status !== 200 ||
    JSON.parse(discovery.text).issuer !== stack.settings.origin
  )
    throw new Error("Canonical HTTPS issuer is not ready.");
  const limiter = JSON.parse(
    (await operator(stack, "limiter-status", [], { capture: true })).stdout,
  );
  const signing = JSON.parse(
    (await operator(stack, "signing-status", [], { capture: true })).stdout,
  );
  if (
    limiter.phase !== "active" ||
    !signing.keys.some((k) => k.phase === "active")
  )
    throw new Error(
      "Signing or shared attempt enforcement is not ready. Follow the explicit initialization/recovery steps.",
    );
  console.log(
    "Verified canonical HTTPS, live PostgreSQL, active signing, and current limiter generation. This is an instantaneous readiness observation.",
  );
}
export async function backup(stack) {
  await stopped(stack);
  const directory = join(
    stack.directory,
    "backups",
    new Date().toISOString().replaceAll(":", "-"),
  );
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const dump = await open(join(directory, "database.dump"), "wx", 0o600);
  try {
    await dumpDatabase(stack, dump.fd);
  } finally {
    await dump.close();
  }
  await cp(join(stack.directory, "secrets"), join(directory, "secrets"), {
    recursive: true,
    errorOnExist: true,
    force: false,
  });
  await cp(join(stack.directory, "pki"), join(directory, "pki"), {
    recursive: true,
    errorOnExist: true,
    force: false,
  });
  await cp(
    join(stack.directory, "settings.json"),
    join(directory, "settings.json"),
    { errorOnExist: true, force: false },
  );
  await writeFile(
    join(directory, "RECOVERY.txt"),
    "Quarantined backup. Do not restore to a serving identity server. Restored credentials can resurrect revoked access. Recovery requires an independently reviewed revocation/reconciliation procedure; see docs/compose.md.\n",
    { mode: 0o600 },
  );
  console.log(
    `Quiesced database and identity-material backup saved to ${directory}. Keep it encrypted and private. Restore-to-service is not automated.`,
  );
  return directory;
}
async function dumpDatabase(stack, fd) {
  await new Promise((resolve, reject) => {
    const child = spawn(
      "docker",
      [
        ...stack.args,
        "exec",
        "-T",
        "postgres",
        "pg_dump",
        "-U",
        "postgres",
        "-d",
        "darkhorse",
        "--format=custom",
        "--no-owner",
        "--no-acl",
      ],
      { env: stack.env, signal: stack.signal, stdio: ["ignore", fd, "ignore"] },
    );
    child.once("error", () =>
      reject(new Error("Cannot start database backup.")),
    );
    child.once("close", (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error(
              "Database backup failed; the incomplete archive is not recoverable.",
            ),
          ),
    );
  });
}
