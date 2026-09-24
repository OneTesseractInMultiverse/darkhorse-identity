import { catalogCommands } from "./lib/catalog-command-test.mjs";
import assert from "node:assert/strict";
import { randomBytes, createHash } from "node:crypto";
import { createServer } from "node:net";
import { readFile, rm, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { run } from "./lib/command.mjs";
import {
  setupStack,
  stackDirectory,
  loadStack,
} from "./lib/deployment-state.mjs";
import {
  compose,
  operator,
  migrate,
  check,
  backup,
} from "./lib/deployment-operations.mjs";
import {
  accountCommand,
  accountResult,
  directoryPage,
} from "./lib/container-account-test.mjs";
import { httpsCall } from "./lib/deployment-client.mjs";
import { validateIdToken, validateCallback } from "./lib/reference-client.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
const name = `verify-${randomBytes(6).toString("hex")}`;
let stack;
const abort = new AbortController();
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
const command = (name, args, options = {}) =>
  run(name, args, { signal: abort.signal, ...options });
async function port() {
  const server = createServer();
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const value = server.address().port;
  await new Promise((r) => server.close(r));
  return value;
}
const captured = { capture: true };
async function sql(statement, acceptFailure = false) {
  return compose(
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
      "-At",
      "-v",
      "ON_ERROR_STOP=1",
    ],
    { input: statement, capture: true, acceptFailure },
  );
}
async function operation(command, args = [], options = {}) {
  return operator(stack, command, args, { ...captured, ...options });
}
async function fixtureRecovery() {
  const state = JSON.parse((await operation("limiter-fence")).stdout);
  assert.ok(state.not_before_ms - state.database_ms >= 903000);
  assert.notEqual(
    (await operation("limiter-activate", [], { acceptFailure: true })).code,
    0,
  );
  // Only this disposable database's clock fixture advances; published commands retain the full wait.
  await sql(
    "ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;",
  );
  const activated = (await operation("limiter-activate")).stdout;
  const id = /Correlation: ([a-f0-9-]{36})/.exec(activated)?.[1];
  assert.ok(id);
  const inspected = await run(
    "make",
    ["--no-print-directory", "--silent", "stack-limiter-inspect"],
    {
      env: { ...stack.env, STACK: stack.settings.name, OPERATION_ID: id },
      capture: true,
    },
  );
  const record = JSON.parse(inspected.stdout);
  assert.equal(record.recorded_outcome, "activated");
  assert.equal(record.database_role, "darkhorse_operator");
  assert.equal(record.same_generation, true);
}
async function prepare() {
  const config = JSON.parse(
    (await compose(stack, ["config", "--format", "json"], captured)).stdout,
  );
  assert.deepEqual(
    Object.keys(config.services).filter(
      (k) => config.services[k].ports?.length,
    ),
    ["edge"],
  );
  assert.ok(
    config.networks.database.internal &&
      config.networks.cache.internal &&
      config.networks.limiter.internal,
  );
  assert.equal(config.services.api.user, "10001:10001");
  assert.ok(config.services.api.read_only);
  assert.ok(
    !config.services.api.secrets.some((s) =>
      /owner|operator|admin/.test(s.source),
    ),
  );
  const operations = JSON.parse(
    (
      await compose(
        stack,
        ["--profile", "operations", "config", "--format", "json"],
        captured,
      )
    ).stdout,
  ).services;
  assert.deepEqual(operations.migrator.secrets.map((s) => s.source).sort(), [
    "ca",
    "owner_db",
  ]);
  assert.deepEqual(Object.keys(operations.migrator.networks), ["database"]);
  assert.ok(
    !operations.operator.secrets.some((s) => /owner|runtime/.test(s.source)),
  );
  assert.ok(
    operations.operator.secrets.some((s) => s.source === "operator_db"),
  );
  assert.deepEqual(operations.account.secrets.map((s) => s.source).sort(), [
    "ca",
    "cache_url",
    "limiter_url",
    "login_key",
    "runtime_db",
  ]);
  assert.deepEqual(Object.keys(operations.account.networks).sort(), [
    "database",
    "limiter",
  ]);
  assert.equal(operations.account.user, "10001:10001");
  assert.ok(operations.account.read_only);
  assert.equal(
    operations.account.environment.DARKHORSE_DATABASE_POOL_SIZE,
    "2",
  );
  await compose(
    stack,
    [
      "up",
      "--detach",
      "--wait",
      "--wait-timeout",
      "90",
      "postgres",
      "cache",
      "limiter",
    ],
    captured,
  );
  console.log("Compose fixture: private dependencies started.");
  await migrate(stack);
  console.log("Compose fixture: schema and runtime grants ready.");
  const migrationId = (
    await sql(
      "SELECT operation_id FROM darkhorse_migration_v1.intents ORDER BY prepared_ms DESC LIMIT 1",
    )
  ).stdout.trim();
  const migrationRecord = JSON.parse(
    (
      await run(
        "make",
        ["--no-print-directory", "--silent", "stack-migration-inspect"],
        {
          env: {
            ...stack.env,
            STACK: stack.settings.name,
            OPERATION_ID: migrationId,
          },
          capture: true,
        },
      )
    ).stdout,
  );
  assert.equal(migrationRecord.recorded_outcome, "completed");
  assert.equal(migrationRecord.database_role, "darkhorse_owner");
  assert.ok(
    migrationRecord.steps.every(
      (v) => v.current_matches && v.completed_ms !== null,
    ),
  );
  assert.notEqual(
    (
      await compose(
        stack,
        [
          "run",
          "--rm",
          "--no-deps",
          "-T",
          "operator",
          "operator",
          "migrate",
          "inspect",
          migrationId,
        ],
        { ...captured, acceptFailure: true },
      )
    ).code,
    0,
  );

  const permissions = (
    await sql(
      "SELECT has_schema_privilege('darkhorse_runtime','public','CREATE'),has_table_privilege('darkhorse_runtime','platform_administrators','INSERT'),has_table_privilege('darkhorse_runtime','signing_keys','INSERT'),has_table_privilege('darkhorse_runtime','limiter_authority','UPDATE'),rolsuper,rolcreaterole FROM pg_roles WHERE rolname='darkhorse_runtime';",
    )
  ).stdout.trim();
  assert.equal(permissions, "f|f|f|f|f|f");
  assert.notEqual(
    (
      await compose(
        stack,
        ["run", "--rm", "--no-deps", "-T", "operator", "--yes", "migrate"],
        { ...captured, acceptFailure: true },
      )
    ).code,
    0,
  );
  const password = randomBytes(24).toString("base64url");
  const boot = await operation("bootstrap", ["--stdin"], {
    input: JSON.stringify({
      email: "compose@example.com",
      first_name: "Compose",
      last_name: "Test",
      password,
    }),
  });
  const principal = boot.stdout.match(/[a-f0-9-]{36}/)[0];
  const stage = JSON.parse((await operation("signing-generate", ["0"])).stdout);
  const inspectedSigning = await run(
    "make",
    ["--no-print-directory", "--silent", "stack-signing-inspect"],
    {
      env: {
        ...stack.env,
        STACK: stack.settings.name,
        OPERATION_ID: stage.operation_id,
      },
      capture: true,
    },
  );
  const signingRecord = JSON.parse(inspectedSigning.stdout);
  assert.equal(signingRecord.recorded_outcome, "completed");
  assert.equal(signingRecord.database_role, "darkhorse_operator");
  assert.equal(signingRecord.current_phase, "staged");
  assert.notEqual(
    (
      await operation("signing-activate", [stage.kid, "1"], {
        acceptFailure: true,
      })
    ).code,
    0,
  );
  // Reuse the explicit disposable signing publication fixture used by the provider suite.
  await sql(
    "ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-60001; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;",
  );
  await operation("signing-activate", [stage.kid, "1"]);
  await fixtureRecovery();
  console.log(
    "Compose fixture: administrator, signing and waited-recovery contracts exercised.",
  );
  return { principal, password };
}
async function transport(origin, ca) {
  const portal = await httpsCall(origin, ca, "/");
  assert.equal(portal.status, 200);
  assert.match(portal.text, /<!doctype html>/i);
  assert.notEqual(
    (
      await httpsCall(origin, ca, "/.well-known/openid-configuration", {
        headers: { host: "api:3001" },
      })
    ).status,
    200,
  );
  await assert.rejects(migrate(stack), /Stop the application/);
  await assert.rejects(backup(stack), /Stop the application/);
  await assert.rejects(httpsCall(origin, undefined, "/health/live"));
  await assert.rejects(
    httpsCall(origin, ca, "/health/live", { servername: "wrong.example" }),
  );
  const inside = await compose(
    stack,
    [
      "exec",
      "-T",
      "edge",
      "curl",
      "--fail",
      "--silent",
      "--show-error",
      "--max-time",
      "5",
      "--cacert",
      "/run/secrets/ca",
      `${origin}/.well-known/openid-configuration`,
    ],
    captured,
  );
  assert.equal(JSON.parse(inside.stdout).issuer, origin);
  const apiId = (
    await compose(stack, ["ps", "-q", "api"], captured)
  ).stdout.trim();
  const inspection = JSON.parse(
    (await command("docker", ["inspect", apiId], captured)).stdout,
  )[0];
  for (const value of inspection.Config.Env)
    assert.ok(
      !/postgres:\/\/|rediss:\/\/|[a-f0-9]{64}$/.test(value),
      "Container environment must contain paths, not credentials",
    );
  for (const args of [
    ["migrate"],
    ["limiter-fence"],
    ["signing-generate", "2"],
  ])
    assert.notEqual(
      (
        await compose(
          stack,
          ["exec", "-T", "api", "darkhorse-server", "--yes", ...args],
          { ...captured, acceptFailure: true },
        )
      ).code,
      0,
    );
  await compose(
    stack,
    [
      "exec",
      "-T",
      "api",
      "sh",
      "-c",
      "test ! -e /run/secrets/owner_db && test ! -e /run/secrets/operator_db && test ! -e /run/secrets/limiter_admin_url && test ! -e /app/context && test ! -e /app/.context",
    ],
    captured,
  );
}
async function sso(origin, ca, principal, password) {
  const http = (path, options = {}) => httpsCall(origin, ca, path, options);
  let cookie = "";
  const browser = async (path, body) => {
    const r = await http(path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        ...(cookie ? { cookie } : {}),
        origin,
        "x-darkhorse-csrf": "1",
        "content-type": "application/json",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    for (const value of r.headers["set-cookie"] ?? []) {
      const pair = value.split(";")[0];
      const key = pair.split("=")[0];
      cookie = [
        ...cookie.split("; ").filter((c) => c && !c.startsWith(`${key}=`)),
        pair,
      ].join("; ");
    }
    return { ...r, body: r.text.startsWith("{") ? JSON.parse(r.text) : null };
  };
  const login = await browser("/api/auth/login", {
    email: "compose@example.com",
    password,
  });
  assert.equal(login.status, 200, "login HTTP status");
  assert.ok(
    login.headers["set-cookie"].some(
      (c) => c.includes("Secure") && c.includes("HttpOnly"),
    ),
  );
  const app = await browser("/api/admin/registration", {
    operation: "create_application",
    application: { name: "Compose client", owner_id: principal, active: true },
  });
  assert.equal(app.status, 200, "app HTTP status");
  const client = await browser("/api/admin/registration", {
    operation: "create_client",
    application_id: app.body.record.id,
    client: {
      name: "Compose client",
      active: true,
      redirect_uris: [`${origin}/callback`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(client.status, 200, "client HTTP status");
  const verifier = randomBytes(32).toString("base64url");
  const expected = {
    issuer: origin,
    client: client.body.record.id,
    subject: principal,
    nonce: randomBytes(24).toString("base64url"),
    state: "compose-state",
    redirect: `${origin}/callback`,
  };
  const query = new URLSearchParams({
    client_id: expected.client,
    response_type: "code",
    redirect_uri: expected.redirect,
    scope: "openid",
    nonce: expected.nonce,
    state: expected.state,
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    code_challenge_method: "S256",
  });
  assert.equal((await browser(`/authorize?${query}`)).status, 303);
  const pending = (await browser("/api/authorization")).body;
  const decision = await browser("/api/authorization/decision", {
    request_id: pending.request_id,
    decision: "approve",
  });
  assert.equal(decision.status, 200, "decision HTTP status");
  const code = validateCallback(decision.body.redirect, expected);
  const authorization = `Basic ${Buffer.from(`${encodeURIComponent(expected.client)}:${encodeURIComponent(client.body.client_secret)}`).toString("base64")}`;
  const issued = await http("/token", {
    method: "POST",
    headers: {
      authorization,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: new URLSearchParams({
      grant_type: "authorization_code",
      code,
      redirect_uri: expected.redirect,
      code_verifier: verifier,
    }).toString(),
  });
  assert.equal(issued.status, 200, "issued HTTP status");
  const tokens = JSON.parse(issued.text);
  assert.match(tokens.access_token, /^da_[a-f0-9]{64}$/);
  const jwks = JSON.parse((await http("/jwks")).text);
  validateIdToken(tokens.id_token, jwks.keys, {
    ...expected,
    now: Math.floor(Date.now() / 1000),
  });
  const introspect = () =>
    http("/introspect", {
      method: "POST",
      headers: {
        authorization,
        "content-type": "application/x-www-form-urlencoded",
      },
      body: new URLSearchParams({ token: tokens.access_token }).toString(),
    });
  assert.equal(JSON.parse((await introspect()).text).active, true);
  assert.equal(
    (
      await http("/userinfo", {
        headers: { authorization: `Bearer ${tokens.access_token}` },
      })
    ).status,
    200,
    "UserInfo HTTP status",
  );
  console.log(
    "Canonical host/container TLS, runtime privilege denials and real OIDC exchange passed.",
  );
  const loginProbe = () =>
    http("/api/auth/login", {
      method: "POST",
      headers: {
        origin,
        "x-darkhorse-csrf": "1",
        "content-type": "application/json",
      },
      body: JSON.stringify({ email: "compose@example.com", password }),
    });
  return { introspect, loginProbe, http };
}
async function outages({ introspect, loginProbe, http }) {
  await compose(stack, ["stop", "cache"], captured);
  assert.equal((await loginProbe()).status, 200);
  assert.equal(JSON.parse((await introspect()).text).active, true);
  await compose(stack, ["start", "cache"], captured);
  await compose(stack, ["restart", "limiter"], captured);
  assert.equal((await loginProbe()).status, 503);
  await fixtureRecovery();
  assert.equal((await loginProbe()).status, 200);
  await compose(stack, ["stop", "postgres"], captured);
  assert.equal((await http("/health/live")).status, 200);
  assert.equal((await introspect()).status, 503);
  await compose(stack, ["up", "--detach", "--wait", "postgres"], captured);
  assert.equal(JSON.parse((await introspect()).text).active, true);
  await compose(stack, ["restart", "api"], captured);
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      if ((await http("/health/live")).status === 200) break;
    } catch {}
    await delay(250, undefined, { signal: abort.signal });
  }
  assert.equal(JSON.parse((await introspect()).text).active, true);
  assert.equal(
    (await sql("SELECT count(*) FROM principals;")).stdout.trim(),
    "1",
  );
}
async function accounts(user) {
  await fixtureRecovery();
  const input = {
    email: "compose@example.com",
    password: user.password,
    reason: "Verify container account commands",
  };
  const settings = { STACK: name, ACCOUNT_ID: user.principal };
  const exec = (extra = {}, auth = input) =>
    accountCommand(command, "compose-exec", { ...settings, ...extra }, auth);
  await runningAccountChecks(exec, user, input);
  await stoppedAccountChecks(settings, input, exec);
  console.log(
    "Account launchers: runtime authentication, confirmation/revision/invariant failures, limiter refusal and lifecycle and directory commands with HTTP stopped passed.",
  );
}
async function runningAccountChecks(exec, user, input) {
  const shown = accountResult(await exec(), 0);
  assert.equal(shown.id, user.principal);
  assert.equal(shown.revision, 0);
  accountResult(
    await exec({}, { ...input, password: "source-defined-wrong-password" }),
    1,
    /Administrator authentication or authority denied/,
  );
  accountResult(
    await exec({ ACCOUNT_OPERATION: "revoke-all", ACCOUNT_REVISION: "0" }),
    3,
    /confirmation_required/,
  );
  accountResult(
    await exec({
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "99",
      ACCOUNT_CONFIRM: "yes",
    }),
    1,
    /principal changed/,
  );
  accountResult(
    await exec({
      ACCOUNT_OPERATION: "deactivate",
      ACCOUNT_REVISION: "0",
      ACCOUNT_CONFIRM: "yes",
    }),
    1,
    /directory invariant/,
  );
  const made = await command(
    "make",
    [
      "--no-print-directory",
      "stack-account-exec",
      `STACK=${name}`,
      `ACCOUNT_ID=${user.principal}`,
      "ACCOUNT_OPERATION=show",
      "ACCOUNT_REVISION=",
      "ACCOUNT_CONFIRM=no",
    ],
    { input: JSON.stringify(input), ...captured },
  );
  assert.equal(accountResult(made, 0).id, user.principal);
  assert.ok(
    !made.stdout.includes(input.password) &&
      !made.stderr.includes(input.password),
  );
  const unconfirmed = await command(
    "make",
    [
      "--no-print-directory",
      "stack-account-exec",
      `STACK=${name}`,
      `ACCOUNT_ID=${user.principal}`,
      "ACCOUNT_OPERATION=revoke-all",
      "ACCOUNT_REVISION=0",
      "ACCOUNT_CONFIRM=no",
    ],
    { input: JSON.stringify(input), ...captured, acceptFailure: true },
  );
  accountResult(unconfirmed, 2, /confirmation_required/);
}
async function stoppedAccountChecks(settings, input, exec) {
  await compose(stack, ["stop", "edge", "api"], captured);
  assert.notEqual((await exec()).code, 0, "exec cannot start a stopped API");
  await compose(stack, ["restart", "limiter"], captured);
  accountResult(
    await accountCommand(command, "compose-run", settings, input),
    1,
    /Account operation unavailable/,
  );
  await fixtureRecovery();
  const target = "00000000-0000-0000-0000-000000000009";
  await sql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','target@example.com','Target','Fixture');`,
  );
  for (const [operation, revision] of [
    ["deactivate", 0],
    ["reactivate", 1],
    ["revoke-all", 2],
  ]) {
    const result = accountResult(
      await accountCommand(
        command,
        "compose-run",
        {
          ...settings,
          ACCOUNT_ID: target,
          ACCOUNT_OPERATION: operation,
          ACCOUNT_REVISION: String(revision),
          ACCOUNT_CONFIRM: "yes",
        },
        input,
      ),
      0,
    );
    assert.equal(result.revision, revision + 1);
    assert.equal(result.changed, true);
  }
  const after = accountResult(
    await accountCommand(
      command,
      "compose-run",
      { ...settings, ACCOUNT_ID: target },
      input,
    ),
    0,
  );
  assert.equal(after.active, true);
  assert.equal(after.revision, 3);
  await directoryPage(
    command,
    "compose-run",
    settings,
    input,
    target,
    "target@example.com",
    sql,
  );
  await catalogCommands(
    (args, input) =>
      compose(
        stack,
        [
          "run",
          "--rm",
          "--no-deps",
          "-T",
          "account",
          "--auth-stdin",
          "--output",
          "json",
          ...args,
        ],
        { ...captured, input, acceptFailure: true },
      ),
    sql,
    settings.ACCOUNT_ID,
    input.password,
  );
  const running = (
    await compose(stack, ["ps", "--services", "--status", "running"], captured)
  ).stdout.split("\n");
  assert.ok(
    !running.some((service) => ["api", "edge", "account"].includes(service)),
  );
  assert.equal(
    (
      await sql(
        "SELECT bool_and(database_role='darkhorse_runtime') FROM operator_account_audit;",
      )
    ).stdout.trim(),
    "t",
  );
}
async function archive() {
  await compose(stack, ["stop", "edge", "api"], captured);
  for (const role of ["operator", "migrator", "account"]) {
    const job = (
      await compose(
        stack,
        [
          "run",
          "--rm",
          "--detach",
          "--no-deps",
          "--entrypoint",
          "/bin/sleep",
          role,
          "60",
        ],
        captured,
      )
    ).stdout.trim();
    try {
      await assert.rejects(backup(stack), /Stop the application/);
      await assert.rejects(migrate(stack), /Stop the application/);
    } finally {
      await command("docker", ["stop", job], {
        ...captured,
        signal: undefined,
      });
    }
  }
  const archive = await backup(stack);
  const bytes = await readFile(join(archive, "database.dump"));
  const inventory = await compose(
    stack,
    ["exec", "-T", "postgres", "pg_restore", "--list"],
    { input: bytes, capture: true },
  );
  assert.match(inventory.stdout, /browser_sessions/);
  // Restore only to a separate quarantined database with no HTTP configuration.
  await compose(
    stack,
    ["exec", "-T", "postgres", "createdb", "-U", "postgres", "quarantine"],
    captured,
  );
  await compose(
    stack,
    [
      "exec",
      "-T",
      "postgres",
      "pg_restore",
      "-U",
      "postgres",
      "-d",
      "quarantine",
      "--exit-on-error",
    ],
    { input: bytes, capture: true },
  );
  const restored = await compose(
    stack,
    [
      "exec",
      "-T",
      "postgres",
      "psql",
      "-U",
      "postgres",
      "-d",
      "quarantine",
      "-Atc",
      "SELECT (SELECT count(*) FROM principals),(SELECT count(*) FROM operator_catalog_audit)",
    ],
    captured,
  );
  assert.equal(restored.stdout.trim(), "3|3");
  console.log(
    "Cache degradation, restrictive limiter restart/recovery, database outage, durable restart and quarantined archive restore passed.",
  );
}
async function main() {
  const origin = `https://identity.localhost:${await port()}`;
  stack = await setupStack(
    name,
    origin,
    process.env.DARKHORSE_TEST_IMAGE ?? "darkhorse:local",
    command,
  );
  stack.signal = abort.signal;
  const manifest = await readFile(
    join(stack.directory, "settings.json"),
    "utf8",
  );
  await writeFile(
    join(stack.directory, "settings.json"),
    JSON.stringify({ ...JSON.parse(manifest), version: 1 }),
  );
  await assert.rejects(loadStack(name), /Incompatible stack manifest/);
  await assert.rejects(
    setupStack(name, origin, stack.settings.image, command),
    /Incompatible stack manifest/,
  );
  assert.equal(
    JSON.parse(await readFile(join(stack.directory, "settings.json"), "utf8"))
      .version,
    1,
  );
  await writeFile(join(stack.directory, "settings.json"), manifest);
  await setupStack(name, origin, stack.settings.image, command);
  await assert.rejects(
    setupStack(
      name,
      "https://other.localhost:9443",
      stack.settings.image,
      command,
    ),
    /already exists/,
  );
  assert.equal(
    await readFile(join(stack.directory, "settings.json"), "utf8"),
    manifest,
  );
  console.log(
    "Compose fixture: isolated credentials, immutable image and private TLS certificates prepared.",
  );
  const { principal, password } = await prepare();
  await compose(
    stack,
    ["up", "--detach", "--wait", "--wait-timeout", "60", "api", "edge"],
    captured,
  );
  await check(stack);
  const ca = await readFile(join(stack.directory, "secrets/ca.pem"));
  await transport(origin, ca);
  const client = await sso(origin, ca, principal, password);
  await outages(client);
  await accounts({ principal, password });
  await archive();
}
try {
  await main();
} catch (error) {
  console.error(error.message);
  if (stack) {
    const state = await compose(stack, ["ps", "--all"], {
      ...captured,
      acceptFailure: true,
      signal: undefined,
    });
    console.error(state.stdout);
    const logs = await compose(stack, ["logs", "--no-color", "--tail", "25"], {
      ...captured,
      acceptFailure: true,
      signal: undefined,
    });
    // Diagnostics are private test output; the services never receive real user data.
    await writeFile(
      ".local/compose-test-services.log",
      logs.stdout + logs.stderr,
      { mode: 0o600 },
    );
  }
  process.exitCode = 1;
} finally {
  if (stack)
    await compose(stack, ["down", "--volumes", "--remove-orphans"], {
      ...captured,
      acceptFailure: true,
      signal: undefined,
    });
  await rm(stackDirectory(name), { recursive: true, force: true });
}
