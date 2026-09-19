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
import { lostReplyProxy, tlsProxy } from "./lib/redis-test-proxy.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
const image =
  "redis:8.10.1@sha256:298e5b3bc566bade82f46ad5511777a4a07a294097ce16ada2f6a42be5239df5";
const prefix = `darkhorse-redis-test-${randomBytes(8).toString("hex")}`;
const owned = [];
const networks = [];
const proxies = [];
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
async function database(network, profiling) {
  const name = `${prefix}-database`;
  const secret = randomBytes(32).toString("hex");
  owned.push(name);
  await docker(
    [
      "run",
      "--detach",
      "--name",
      name,
      "--network",
      network,
      "--publish",
      "127.0.0.1::5432",
      "--env",
      "POSTGRES_PASSWORD",
      "percona/percona-distribution-postgresql:18.6@sha256:dae47360e8137cafc1e8d66f9a1be348f1405e3cf51daa383b94e6c277e6b256",
      ...(profiling
        ? [
            "postgres",
            "-c",
            "shared_preload_libraries=pg_stat_statements",
            "-c",
            "pg_stat_statements.track=top",
            "-c",
            "pg_stat_statements.track_planning=off",
            "-c",
            "pg_stat_statements.track_utility=on",
          ]
        : []),
    ],
    { env: { ...process.env, POSTGRES_PASSWORD: secret } },
  );
  let available = false;
  for (let i = 0; i < 60; i++) {
    if (
      (
        await docker(
          ["exec", name, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
          {
            acceptFailure: true,
          },
        )
      ).code === 0
    ) {
      available = true;
      break;
    }
    await delay(500, undefined, { signal: abort.signal });
  }
  if (!available) throw new Error("Temporary database did not become ready.");
  const port = (await docker(["port", name, "5432/tcp"])).stdout
    .trim()
    .split(":")
    .at(-1);
  return {
    name,
    url: `postgres://postgres:${secret}@127.0.0.1:${port}/postgres`,
  };
}
async function hostChecks(env, db, directory, benchmark) {
  const profiling = benchmark?.profile.profiling === true;
  if (!benchmark) {
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
    await command(
      "cargo",
      [
        "test",
        "-p",
        "darkhorse-adapters",
        "--features",
        "redis-tests",
        "--test",
        "limiter",
        "--locked",
        "--offline",
        "--",
        "--nocapture",
      ],
      { env },
    );
  }
  await command("cargo", [
    "build",
    ...(benchmark ? ["--release"] : []),
    ...(profiling ? ["--features", "benchmark-profiling"] : []),
    "-p",
    "darkhorse-server",
    "--locked",
    "--offline",
  ]);
  const executable = resolve(
    process.env.CARGO_TARGET_DIR ?? "target",
    `${benchmark ? "release" : "debug"}/darkhorse-server`,
  );
  const invoke = (operation, operator = false, acceptFailure = false) =>
    command(executable, [operation], {
      env: {
        ...runtimeEnvironment(env),
        ...(operator
          ? {
              DARKHORSE_REDIS_LIMITER_ADMIN_URL:
                env.DARKHORSE_REDIS_LIMITER_ADMIN_URL,
            }
          : {}),
      },
      capture: true,
      acceptFailure,
    });
  await verifyLimiter(invoke, db);
  if (benchmark || process.env.DARKHORSE_TEST_BROWSER === "true") {
    const { verifyBrowser } = await import("./browser-test.mjs");
    const options = benchmark
      ? {
          profile: "release",
          profiling,
          poolSize: benchmark.poolSize,
          exercise: async (options) =>
            (await import("./lib/benchmark-browser.mjs")).benchmarkBrowser(
              options,
              benchmark.profile,
            ),
        }
      : undefined;
    await verifyBrowser(env, db, directory, command, docker, options);
  }
  return invoke("redis-status");
}
async function verifyLimiter(invoke, db) {
  await invoke("migrate");
  const cooling = JSON.parse((await invoke("limiter-fence")).stdout);
  assert.equal(cooling.phase, "cooling");
  assert.ok(cooling.not_before_ms - cooling.database_ms >= 903000);
  assert.equal(
    JSON.parse((await invoke("limiter-status")).stdout).phase,
    "cooling",
  );
  const early = await invoke("limiter-activate", true, true);
  assert.notEqual(early.code, 0);
  assert.match(early.stderr, /recovery wait/);
  // Advance only the owner-controlled disposable SQL fixture; no runtime bypass exists.
  await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-v",
    "ON_ERROR_STOP=1",
    "-c",
    "ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;",
  ]);
  assert.match(
    (await invoke("limiter-activate", true)).stdout,
    /generation activated/,
  );
  const active = JSON.parse((await invoke("limiter-status")).stdout);
  assert.equal(active.phase, "active");
  assert.equal(active.counter_entries, 0);
  assert.notEqual((await invoke("limiter-activate", true, true)).code, 0);
  console.log(
    "Operator fencing, mandatory wait, activation and authoritative status checks passed.",
  );
}

function containerUrl(raw, name) {
  const url = new URL(raw);
  url.hostname = name;
  url.port = "6379";
  return url.href;
}
async function imageChecks(tag, env, cache, limiter, db) {
  const base = {
    ...runtimeEnvironment(env),
    DARKHORSE_REDIS_CACHE_URL: containerUrl(
      env.DARKHORSE_REDIS_CACHE_URL,
      cache.name,
    ),
    DARKHORSE_REDIS_LIMITER_URL: containerUrl(
      env.DARKHORSE_REDIS_LIMITER_URL,
      limiter.name,
    ),
  };
  const databaseUrl = new URL(db.url);
  databaseUrl.hostname = db.name;
  databaseUrl.port = "5432";
  base.DARKHORSE_DATABASE_URL = databaseUrl.href;
  const invoke = async (operation, operator = false, acceptFailure = false) => {
    const name = `${prefix}-probe-${owned.length}`;
    owned.push(name);
    const runtime = {
      ...base,
      ...(operator
        ? {
            DARKHORSE_REDIS_LIMITER_ADMIN_URL: containerUrl(
              env.DARKHORSE_REDIS_LIMITER_ADMIN_URL,
              limiter.name,
            ),
          }
        : {}),
    };
    const names = [
      "DARKHORSE_REDIS_CACHE_URL",
      "DARKHORSE_REDIS_LIMITER_URL",
      "DARKHORSE_REDIS_INSECURE",
      "DARKHORSE_DATABASE_URL",
      "DARKHORSE_DATABASE_INSECURE",
      ...(operator ? ["DARKHORSE_REDIS_LIMITER_ADMIN_URL"] : []),
    ];
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
        ...names.flatMap((n) => ["--env", n]),
        tag,
        operation,
      ],
      { env: runtime },
    );
    await docker(["network", "connect", limiter.network, name]);
    return docker(["start", "--attach", name], { acceptFailure });
  };
  await verifyLimiter(invoke, db);
  return invoke("redis-status");
}

async function main(args) {
  const benchmark = args.length === 1 && args[0] === "--benchmark";
  let settings;
  if (benchmark) {
    const { benchmarkSettings } = await import("./lib/benchmark-settings.mjs");
    settings = benchmarkSettings(process.env);
  }
  if (
    args.length &&
    !benchmark &&
    !(
      args.length === 2 &&
      args[0] === "--image" &&
      args[1] &&
      !args[1].startsWith("-")
    )
  )
    throw new Error("Usage: redis-test.mjs [--image IMAGE | --benchmark]");
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
    const fault = !args.length
      ? await lostReplyProxy(Number(limiter.port))
      : null;
    if (fault) proxies.push(fault);
    const stalled = !args.length
      ? await lostReplyProxy(Number(limiter.port), false)
      : null;
    if (stalled) proxies.push(stalled);
    const tls = !args.length
      ? await tlsProxy(Number(limiter.port), directory, command)
      : null;
    if (tls) proxies.push(tls);
    const db = await database(
      cache.network,
      settings?.profile.profiling === true,
    );
    const env = {
      ...process.env,
      ...values,
      ...(!args.length
        ? {
            DARKHORSE_TEST_REDIS_DROP_PORT: String(fault.port),
            DARKHORSE_TEST_REDIS_TIMEOUT_PORT: String(stalled.port),
            DARKHORSE_TEST_REDIS_TLS_PORT: String(tls.port),
            DARKHORSE_TEST_REDIS_CA_PEM: tls.ca,
          }
        : {}),
      DARKHORSE_TEST_DATABASE_URL: db.url,
      DARKHORSE_DATABASE_URL: db.url,
      DARKHORSE_DATABASE_INSECURE: "true",
      DARKHORSE_TEST_REDIS_CACHE_CONTAINER: cache.name,
      DARKHORSE_TEST_REDIS_LIMITER_CONTAINER: limiter.name,
    };
    const result =
      args.length && !benchmark
        ? await imageChecks(args[1], env, cache, limiter, db)
        : await hostChecks(env, db, directory, settings);
    const status = JSON.parse(result.stdout);
    assert.equal(status.cache.connection, "reachable");
    assert.equal(status.limiter.connection, "reachable");
    assert.equal(status.shared_enforcement, "not_checked");
    for (const secret of secrets)
      assert.ok(!`${result.stdout}${result.stderr}`.includes(secret));
    console.log(
      benchmark
        ? "Release benchmark completed; reports saved under .local/benchmarks."
        : args.length
          ? "Packaged Redis diagnostics passed; login integration remains separate."
          : "Redis infrastructure, shared enforcement and password login integration passed.",
    );
  } finally {
    for (const proxy of proxies.reverse()) await proxy.close();
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
