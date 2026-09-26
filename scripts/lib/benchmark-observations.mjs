import { redisMetrics } from "./benchmark-redis-model.mjs";
import {
  cpus,
  totalmem,
  platform,
  release,
  arch,
  availableParallelism,
} from "node:os";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

export async function metadata({
  command,
  docker,
  db,
  env,
  executable,
  profileSnapshot,
  poolSize,
  databaseRole,
}) {
  const capture = async (program, args) =>
    (await command(program, args, { capture: true })).stdout.trim();
  const paths = (
    await capture("git", [
      "ls-files",
      "--cached",
      "--others",
      "--exclude-standard",
      "--",
      "apps",
      "crates",
      "scripts",
      "config",
      "Makefile",
      "Cargo.toml",
      "Cargo.lock",
      "package.json",
      "pnpm-lock.yaml",
      "rust-toolchain.toml",
    ])
  )
    .split("\n")
    .filter(Boolean)
    .sort();
  const hash = createHash("sha256");
  for (const path of paths) {
    hash
      .update(path)
      .update("\0")
      .update(await readFile(path))
      .update("\0");
  }
  const images = {};
  for (const [label, name] of Object.entries({
    database: db.name,
    cache: env.DARKHORSE_TEST_REDIS_CACHE_CONTAINER,
    limiter: env.DARKHORSE_TEST_REDIS_LIMITER_CONTAINER,
  }))
    images[label] = (
      await docker(["inspect", "--format", "{{.Config.Image}}", name])
    ).stdout.trim();
  return {
    commit: await capture("git", ["rev-parse", "HEAD"]),
    sourceSha256: hash.digest("hex"),
    binarySha256: createHash("sha256")
      .update(await readFile(executable))
      .digest("hex"),
    sourceDirty:
      (
        await capture("git", [
          "status",
          "--porcelain",
          "--untracked-files=normal",
          "--",
          ...paths,
        ])
      ).length > 0,
    rust: await capture("rustc", ["--version"]),
    node: process.version,
    database: (
      await docker(["exec", db.name, "postgres", "--version"])
    ).stdout.trim(),
    images,
    host: {
      platform: platform(),
      release: release(),
      arch: arch(),
      cpu: cpus()[0]?.model,
      logicalCpus: cpus().length,
      availableParallelism: availableParallelism(),
      memoryBytes: totalmem(),
    },
    docker: JSON.parse(
      (
        await docker([
          "info",
          "--format",
          '{"version":{{json .ServerVersion}},"cpus":{{json .NCPU}},"memoryBytes":{{json .MemTotal}},"arch":{{json .Architecture}}}',
        ])
      ).stdout,
    ),
    topology:
      "One host release Rust process, Node TLS proxy and load generator; Docker Percona primary plus separate Redis cache/limiter containers on loopback. Chromium provisions real login/SSO credentials.",
    buildFeatures: profileSnapshot ? ["benchmark-profiling"] : [],
    protections: {
      httpsVerification: true,
      clientAuthentication: "client_secret_basic",
      loginSharedRedisLimiter: true,
      introspectionSharedRedisLimiter: true,
      introspectionBudgets: {
        windowMs: 60_000,
        global: Number(env.DARKHORSE_INTROSPECTION_GLOBAL_PER_MINUTE ?? 60_000),
        caller: Number(env.DARKHORSE_INTROSPECTION_CALLER_PER_MINUTE ?? 6_000),
        separateLocalPool: true,
        globalQueueCapacity: 16,
        globalUpdateConcurrency: 2,
        queueIncludedInDeadlineMs: 1000,
      },
      tokenRouteConcurrency: 16,
      databasePoolConnections: poolSize,
      positiveDecisionCache: false,
      computationCache: false,
      databaseRole,
    },
    unmeasured: [
      "database-cold state",
      "multi-host latency",
      "large policy populations",
      ...(profileSnapshot
        ? []
        : [
            "pool acquisition and authorization stage timings",
            "SQL statement counts and WAL observations",
          ]),
      "SQL round trips and plans",
      "limiter incremental overhead",
      "multi-hour endurance and production arrival distributions",
      "sustained CPU/RSS peaks",
    ],
  };
}
export async function snapshot({ command, docker, db, serverPid, env }) {
  const sql = `SELECT json_build_object(
    'database', (SELECT row_to_json(s) FROM (SELECT xact_commit,xact_rollback,blks_read,blks_hit,tup_returned,tup_fetched,tup_inserted,tup_updated,tup_deleted,deadlocks,temp_bytes FROM pg_stat_database WHERE datname=current_database()) s),
    'activity', (SELECT json_agg(s) FROM (SELECT state,wait_event_type,count(*) FROM pg_stat_activity WHERE datname=current_database() AND application_name='darkhorse' GROUP BY state,wait_event_type) s),
    'waiting_locks', (SELECT count(*) FROM pg_locks WHERE NOT granted AND pid IN (SELECT pid FROM pg_stat_activity WHERE datname=current_database())));`;
  const database = await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-d",
    "browser_test",
    "-v",
    "ON_ERROR_STOP=1",
    "-tAc",
    sql,
  ]);
  const server = await command(
    "ps",
    ["-p", String(serverPid), "-o", "time=,rss="],
    { capture: true },
  );
  const containers = await docker([
    "stats",
    "--no-stream",
    "--format",
    "{{json .}}",
    db.name,
  ]);
  const row = JSON.parse(containers.stdout.trim());
  return {
    redis: await redisSnapshots(docker, env),
    database: JSON.parse(database.stdout),
    server: { cpuTimeAndRssKiB: server.stdout.trim() },
    databaseContainer: {
      cpu: row.CPUPerc,
      memory: row.MemUsage,
      blockIO: row.BlockIO,
    },
  };
}

async function redisSnapshots(docker, env) {
  const result = {};
  for (const [role, key] of [
    ["cache", "CACHE"],
    ["limiter", "LIMITER"],
  ]) {
    const url = new URL(env[`DARKHORSE_REDIS_${key}_URL`]);
    const observed = await docker(
      [
        "exec",
        "--env",
        "REDISCLI_AUTH",
        env[`DARKHORSE_TEST_REDIS_${key}_CONTAINER`],
        "redis-cli",
        "--user",
        decodeURIComponent(url.username),
        "--no-auth-warning",
        "--raw",
        "INFO",
        "all",
      ],
      {
        env: {
          ...process.env,
          REDISCLI_AUTH: decodeURIComponent(url.password),
        },
      },
    );
    result[role] = redisMetrics(observed.stdout);
  }
  return result;
}
