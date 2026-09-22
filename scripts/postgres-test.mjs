import { randomBytes } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { resolve } from "node:path";
import assert from "node:assert/strict";
import { run } from "./lib/command.mjs";
import { verifySecretFiles } from "./lib/configuration-file-test.mjs";

process.chdir(resolve(import.meta.dirname, ".."));
const image =
  "percona/percona-distribution-postgresql:18.6@sha256:dae47360e8137cafc1e8d66f9a1be348f1405e3cf51daa383b94e6c277e6b256";
const suffix = randomBytes(8).toString("hex");
const database = `darkhorse-test-${suffix}`;
const network = `darkhorse-net-${suffix}`;
const server = `darkhorse-server-${suffix}`;
const password = randomBytes(32).toString("hex");
const abort = new AbortController();
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
const command = (name, args, options = {}) =>
  run(name, args, { signal: abort.signal, ...options });
const docker = (args, options = {}) =>
  command("docker", args, { capture: true, ...options });
const owned = [];

async function ready() {
  for (let attempt = 0; attempt < 60; attempt++) {
    const result = await docker(
      ["exec", database, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
      { acceptFailure: true },
    );
    if (result.code === 0) return;
    await delay(500, undefined, { signal: abort.signal });
  }
  throw new Error("Disposable PostgreSQL did not become ready.");
}

async function verifyOperator(invoke) {
  assert.match((await invoke(["--help"])).stdout, /operator/);
  const invalid = await invoke(["bootstrap", "--stdin"], "{}", true);
  assert.notEqual(invalid.code, 0);
  assert.match(invalid.stderr, /Invalid bootstrap input/);
  assert.match((await invoke(["migrate"])).stdout, /migrations applied/);
  const input = JSON.stringify({
    email: "smoke@example.com",
    first_name: "Smoke",
    last_name: "Test",
    password: randomBytes(24).toString("base64url"),
  });
  const first = await invoke(["bootstrap", "--stdin"], input);
  const match = first.stdout.match(
    /Administrator initialized: ([a-f0-9-]{36})/,
  );
  assert.ok(match);
  const second = await invoke(["bootstrap", "--stdin"], input, true);
  assert.notEqual(second.code, 0);
  assert.match(second.stderr, /already complete/);
  const record = JSON.parse((await invoke(["account", match[1]])).stdout);
  assert.equal(record.email, "smoke@example.com");
  assert.equal(record.platform_administrator, true);
  const denied = await invoke(["deactivate", match[1], "0"], undefined, true);
  assert.notEqual(denied.code, 0);
  await invoke(["revoke-all", match[1], "0"]);
  const updated = JSON.parse((await invoke(["account", match[1]])).stdout);
  assert.equal(updated.credential_epoch, 1);
  assert.equal(updated.revision, 1);
  const structured = await invoke([
    "--output",
    "json",
    "operator",
    "account",
    "show",
    match[1],
  ]);
  assert.equal(structured.stderr, "");
  const envelope = JSON.parse(structured.stdout);
  assert.equal(envelope.schema_version, 1);
  assert.equal(envelope.ok, true);
  assert.deepEqual(envelope.data, updated);
  const rejected = await invoke(
    ["--output", "json", "operator", "account", "revoke-all", match[1], "0"],
    undefined,
    true,
  );
  assert.equal(rejected.code, 1);
  assert.equal(rejected.stdout, "");
  assert.equal(JSON.parse(rejected.stderr).error.code, "operation_failed");
  const conflict = await invoke(["revoke-all", match[1], "0"], undefined, true);
  assert.notEqual(conflict.code, 0);
  assert.match(conflict.stderr, /changed/);
  for (const result of [first, second, invalid, denied, conflict])
    assert.ok(
      !`${result.stdout}${result.stderr}`.includes(JSON.parse(input).password),
    );
  console.log(
    "Operator migration/bootstrap/duplicate/last-administrator/revocation checks passed.",
  );
}

async function hostChecks(url) {
  const env = {
    ...process.env,
    DARKHORSE_TEST_DATABASE_URL: url,
    DARKHORSE_DATABASE_URL: url,
    DARKHORSE_DATABASE_INSECURE: "true",
  };
  await command(
    "cargo",
    [
      "test",
      "-p",
      "darkhorse-adapters",
      "--features",
      "postgres-tests",
      "--test",
      "postgres",
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
  const executable = resolve(
    process.env.CARGO_TARGET_DIR ?? "target",
    "debug/darkhorse-server",
  );
  await verifySecretFiles(executable, env, command);
  const secure = await command(executable, ["migrate", "--yes"], {
    env: { ...env, DARKHORSE_DATABASE_INSECURE: "false" },
    input: "",
    capture: true,
    acceptFailure: true,
  });
  assert.notEqual(
    secure.code,
    0,
    "TLS-required connection must reject a plaintext database",
  );
  assert.ok(
    !secure.stderr.includes(url),
    "connection errors must not expose credentials",
  );
  await verifyOperator((args, input, acceptFailure = false) =>
    command(executable, ["--yes", ...args], {
      env,
      input,
      acceptFailure,
      capture: true,
    }),
  );
}

async function imageChecks(tag) {
  const env = {
    ...process.env,
    DARKHORSE_DATABASE_URL: `postgres://postgres:${password}@${database}:5432/postgres`,
    DARKHORSE_DATABASE_INSECURE: "true",
  };
  await verifyOperator((args, input, acceptFailure = false) =>
    docker(
      [
        "run",
        "--rm",
        "--interactive",
        "--network",
        network,
        "--env",
        "DARKHORSE_DATABASE_URL",
        "--env",
        "DARKHORSE_DATABASE_INSECURE",
        tag,
        "--yes",
        ...args,
      ],
      { env, input, acceptFailure },
    ),
  );
  owned.push(server);
  await docker([
    "run",
    "--detach",
    "--rm",
    "--name",
    server,
    "--network",
    network,
    "--read-only",
    "--cap-drop",
    "ALL",
    "--security-opt",
    "no-new-privileges",
    "--publish",
    "127.0.0.1::3001",
    tag,
  ]);
  const port = (await docker(["port", server, "3001/tcp"])).stdout
    .trim()
    .split(":")
    .at(-1);
  let healthy = false;
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      healthy = (
        await fetch(`http://127.0.0.1:${port}/health/live`, {
          signal: AbortSignal.timeout(1000),
        })
      ).ok;
    } catch {}
    if (healthy) break;
    await delay(200, undefined, { signal: abort.signal });
  }
  assert.ok(healthy, "image must become healthy");
  const page = await fetch(`http://127.0.0.1:${port}/`);
  assert.match(await page.text(), /Darkhorse/);
  for (const route of [
    "console/applications",
    "console/clients",
    "console/resources",
    "console/scopes",
    "console/roles",
    "console/capabilities",
    "security/keys",
    "account/profile",
    "console/profile",
    "console/settings",
  ]) {
    const response = await fetch(`http://127.0.0.1:${port}/${route}`);
    assert.equal(response.status, 200);
    assert.match(response.headers.get("content-type"), /text\/html/);
    assert.match(await response.text(), /Darkhorse/);
  }
  assert.equal(
    (await docker(["exec", server, "id", "-u"])).stdout.trim(),
    "10001",
  );
  const privateFiles = await docker(
    [
      "exec",
      server,
      "sh",
      "-c",
      "test ! -e /app/.context && test ! -e /app/context && test ! -e /app/.local && test ! -e /build && test ! -e /root/.cargo",
    ],
    { acceptFailure: true },
  );
  assert.equal(privateFiles.code, 0);
  console.log(
    "Image HTTP/static/non-root/read-only/private-input checks passed.",
  );
}

async function main() {
  let createdNetwork = false;
  try {
    await docker(["network", "create", network]);
    createdNetwork = true;
    owned.push(database);
    await docker(
      [
        "run",
        "--detach",
        "--rm",
        "--name",
        database,
        "--network",
        network,
        "--publish",
        "127.0.0.1::5432",
        "--env",
        "POSTGRES_PASSWORD",
        "--env",
        "POSTGRES_INITDB_ARGS=--encoding=UTF8",
        image,
      ],
      { env: { ...process.env, POSTGRES_PASSWORD: password } },
    );
    await ready();
    const [mode, tag] = process.argv.slice(2);
    if (mode === "--image" && tag) await imageChecks(tag);
    else if (mode === undefined) {
      const port = (await docker(["port", database, "5432/tcp"])).stdout
        .trim()
        .split(":")
        .at(-1);
      await hostChecks(
        `postgres://postgres:${password}@127.0.0.1:${port}/postgres`,
      );
    } else throw new Error("Use no arguments, or --image IMAGE.");
  } finally {
    for (const name of owned.reverse())
      await run("docker", ["rm", "--force", name], {
        capture: true,
        acceptFailure: true,
      });
    if (createdNetwork)
      await run("docker", ["network", "rm", network], {
        capture: true,
        acceptFailure: true,
      });
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
