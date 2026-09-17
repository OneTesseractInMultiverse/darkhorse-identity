import { randomBytes } from "node:crypto";
import { createServer } from "node:net";
import { mkdir, mkdtemp, writeFile, rm } from "node:fs/promises";
import { resolve, join, basename } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import assert from "node:assert/strict";
import {
  contents,
  parseEnvironment,
  runtimeEnvironment,
} from "./lib/redis-settings.mjs";
import { run } from "./lib/command.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
const image =
  "redis:8.10.1@sha256:298e5b3bc566bade82f46ad5511777a4a07a294097ce16ada2f6a42be5239df5";
const prefix = `darkhorse-redis-test-${randomBytes(8).toString("hex")}`;
const owned = [];
const networks = [];
const abort = new AbortController();
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
const command = (name, args, options = {}) =>
  run(name, args, { signal: abort.signal, ...options });
const docker = (args, options = {}) =>
  command("docker", args, { capture: true, ...options });
async function ready(name) {
  for (let i = 0; i < 60; i++) {
    const result = await docker(["exec", name, "redis-cli", "--raw", "ping"], {
      acceptFailure: true,
    });
    if (result.stdout.includes("NOAUTH")) return;
    await delay(500, undefined, { signal: abort.signal });
  }
  throw new Error("Temporary Redis did not become ready.");
}
async function availablePort() {
  const listener = createServer();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolve);
  });
  const port = listener.address().port;
  await new Promise((resolve, reject) =>
    listener.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}
async function service(role, directory) {
  const name = `${prefix}-${role}`,
    network = `${name}-net`;
  await docker(["network", "create", network]);
  networks.push(network);
  owned.push(name);
  const requestedPort = await availablePort();
  await docker([
    "run",
    "--detach",
    "--name",
    name,
    "--network",
    network,
    "--publish",
    `127.0.0.1:${requestedPort}:6379`,
    "--tmpfs",
    "/run/redis",
    "--tmpfs",
    "/data",
    "--volume",
    `${resolve("config/redis", `${role}.conf`)}:/usr/local/etc/redis/redis.conf:ro`,
    "--volume",
    `${resolve("scripts/redis-entrypoint.sh")}:/opt/redis-entrypoint.sh:ro`,
    "--volume",
    `${join(directory, `redis-${role}.acl`)}:/run/secrets/redis_acl:ro`,
    "--entrypoint",
    "sh",
    image,
    "/opt/redis-entrypoint.sh",
  ]);
  await ready(name);
  const port = (await docker(["port", name, "6379/tcp"])).stdout
    .trim()
    .split(":")
    .at(-1);
  return { name, port, network };
}
async function hostChecks(env) {
  await command(
    "cargo",
    [
      "test",
      "-p",
      "darkhorse-adapters",
      "--features",
      "redis-tests",
      "--test",
      "redis",
      "--locked",
      "--offline",
    ],
    { env },
  );
  await command("cargo", [
    "build",
    "-p",
    "darkhorse-server",
    "--locked",
    "--offline",
  ]);
  return command(
    resolve(process.env.CARGO_TARGET_DIR ?? "target", "debug/darkhorse-server"),
    ["redis-status"],
    { env: runtimeEnvironment(env), capture: true },
  );
}
function containerUrl(raw, name) {
  const url = new URL(raw);
  url.hostname = name;
  url.port = "6379";
  return url.href;
}
async function imageChecks(tag, env, cache, limiter) {
  const name = `${prefix}-probe`;
  owned.push(name);
  const runtime = runtimeEnvironment(env);
  runtime.DARKHORSE_REDIS_CACHE_URL = containerUrl(
    runtime.DARKHORSE_REDIS_CACHE_URL,
    cache.name,
  );
  runtime.DARKHORSE_REDIS_LIMITER_URL = containerUrl(
    runtime.DARKHORSE_REDIS_LIMITER_URL,
    limiter.name,
  );
  await docker(
    [
      "create",
      "--name",
      name,
      "--read-only",
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges",
      "--network",
      cache.network,
      "--env",
      "DARKHORSE_REDIS_CACHE_URL",
      "--env",
      "DARKHORSE_REDIS_LIMITER_URL",
      "--env",
      "DARKHORSE_REDIS_INSECURE",
      tag,
      "redis-status",
    ],
    { env: runtime },
  );
  await docker(["network", "connect", limiter.network, name]);
  return docker(["start", "--attach", name]);
}
async function main(args) {
  if (
    args.length &&
    !(
      args.length === 2 &&
      args[0] === "--image" &&
      args[1] &&
      !args[1].startsWith("-")
    )
  )
    throw new Error("Usage: redis-test.mjs [--image IMAGE]");
  await mkdir(".local", { recursive: true, mode: 0o700 });
  const directory = await mkdtemp(resolve(".local/redis-test-"));
  try {
    const secrets = Array.from({ length: 4 }, () =>
      randomBytes(32).toString("hex"),
    );
    const files = contents(secrets);
    for (const [path, text] of Object.entries(files))
      if (path.endsWith(".acl"))
        await writeFile(join(directory, basename(path)), text, { mode: 0o600 });
    const cache = await service("cache", directory),
      limiter = await service("limiter", directory);
    const values = parseEnvironment(
      contents(secrets, [Number(cache.port), Number(limiter.port)])[
        ".local/redis.env"
      ],
    );
    const env = {
      ...process.env,
      ...values,
      DARKHORSE_TEST_REDIS_CACHE_CONTAINER: cache.name,
      DARKHORSE_TEST_REDIS_LIMITER_CONTAINER: limiter.name,
    };
    const result = args.length
      ? await imageChecks(args[1], env, cache, limiter)
      : await hostChecks(env);
    const status = JSON.parse(result.stdout);
    assert.equal(status.cache.connection, "reachable");
    assert.equal(status.limiter.connection, "reachable");
    assert.equal(status.shared_enforcement, "not_configured");
    for (const secret of secrets)
      assert.ok(!`${result.stdout}${result.stderr}`.includes(secret));
    console.log(
      args.length
        ? "Packaged Redis diagnostics passed; live admission remains unconfigured."
        : "Redis role isolation, infrastructure faults and redacted operator diagnostics passed; live admission remains unconfigured.",
    );
  } finally {
    for (const name of owned.reverse())
      await run("docker", ["rm", "--force", "--volumes", name], {
        capture: true,
        acceptFailure: true,
      });
    for (const name of networks.reverse())
      await run("docker", ["network", "rm", name], {
        capture: true,
        acceptFailure: true,
      });
    await rm(directory, { recursive: true, force: true });
  }
}
main(process.argv.slice(2)).catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
