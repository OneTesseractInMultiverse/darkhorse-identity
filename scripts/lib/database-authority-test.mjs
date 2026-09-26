// Real database/process evidence; never imported by the service-free unit suite.
import { verifyOperatorAuthority } from "./operator-authority-test.mjs";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function verifyDatabaseAuthority(docker, database, command, port) {
  const fixture = await prepare(docker, database, command, port);
  const grants = await readFile("deploy/grant-runtime.sql", "utf8");
  await verifyGrants(fixture, grants);
  await verifyAudit(fixture, grants);
  await verifyTopology(fixture, grants);
  await verifyAtomicGrants(fixture, grants);
  await verifyOperatorAuthority(fixture, grants);
  console.log(
    "Runtime allowlist, reapplication, new-object denial, role topology, audit rollback and atomic grants passed with authenticated nonowner credentials.",
  );
}

async function prepare(docker, database, command, port) {
  const passwords = {
    darkhorse_owner: randomBytes(32).toString("hex"),
    darkhorse_operator: randomBytes(32).toString("hex"),
    darkhorse_runtime: randomBytes(32).toString("hex"),
  };
  const admin = (input, name = "darkhorse_authority") =>
    docker(
      [
        "exec",
        "--interactive",
        database,
        "psql",
        "-X",
        "-U",
        "postgres",
        "-d",
        name,
        "-At",
        "-v",
        "ON_ERROR_STOP=1",
      ],
      { input },
    );
  const sql = async (
    role,
    input,
    acceptFailure = false,
    password = passwords[role],
  ) => {
    const result = await docker(
      [
        "exec",
        "--interactive",
        "--env",
        "PGPASSWORD",
        database,
        "psql",
        "-X",
        "-h",
        "127.0.0.1",
        "-U",
        role,
        "-d",
        "darkhorse_authority",
        "-At",
        "-v",
        "ON_ERROR_STOP=1",
      ],
      {
        env: { ...process.env, PGPASSWORD: password },
        input,
        acceptFailure: true,
      },
    );
    if (!acceptFailure)
      assert.equal(result.code, 0, `Authority SQL failed: ${result.stderr}`);
    return result;
  };
  // Initial provisioning uses only this disposable container's trusted socket.
  await admin(
    `CREATE ROLE darkhorse_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '${passwords.darkhorse_owner}';
CREATE ROLE darkhorse_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '${passwords.darkhorse_runtime}';
CREATE ROLE darkhorse_operator LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '${passwords.darkhorse_operator}';
CREATE DATABASE darkhorse_authority OWNER darkhorse_owner;
REVOKE ALL ON DATABASE darkhorse_authority FROM PUBLIC;
GRANT CONNECT ON DATABASE darkhorse_authority TO darkhorse_runtime,darkhorse_owner,darkhorse_operator;`,
    "postgres",
  );
  const executable = resolve(
    process.env.CARGO_TARGET_DIR ?? "target",
    "debug/darkhorse-server",
  );
  const invoke = (role, args, acceptFailure = false, options = {}) =>
    command(executable, ["--yes", ...args], {
      env: {
        ...process.env,
        DARKHORSE_DATABASE_URL: `postgres://${role}:${passwords[role]}@127.0.0.1:${port}/darkhorse_authority`,
        DARKHORSE_DATABASE_INSECURE: "true",
        ...options.env,
      },
      input: options.input ?? "",
      capture: true,
      acceptFailure,
    });
  const migrated = JSON.parse(
    (
      await invoke("darkhorse_owner", [
        "--output",
        "json",
        "operator",
        "migrate",
      ])
    ).stdout,
  ).data;
  const inspectArgs = [
    "--output",
    "json",
    "operator",
    "migrate",
    "inspect",
    migrated.operation_id,
  ];
  const inspected = JSON.parse(
    (await invoke("darkhorse_owner", inspectArgs)).stdout,
  ).data;
  assert.equal(inspected.recorded_outcome, "completed");
  assert.equal(inspected.database_role, "darkhorse_owner");
  const absent = JSON.parse(
    (
      await invoke("darkhorse_owner", [
        "--output",
        "json",
        "operator",
        "migrate",
        "inspect",
        "00000000-0000-0000-0000-000000000123",
      ])
    ).stdout,
  ).data;
  assert.equal(absent.recorded_outcome, "absent");
  assert.equal(inspected.steps.length, 34);
  assert.ok(
    inspected.steps.every(
      (v) => v.completed_ms !== null && !v.already_applied && v.current_matches,
    ),
  );
  for (const role of ["darkhorse_runtime", "darkhorse_operator"]) {
    assert.notEqual((await invoke(role, inspectArgs, true)).code, 0);
    assert.notEqual(
      (await sql(role, "SELECT * FROM darkhorse_migration_v1.intents", true))
        .code,
      0,
    );
  }
  const invalid = await sql(
    "darkhorse_runtime",
    "SELECT 1;",
    true,
    randomBytes(32).toString("hex"),
  );
  assert.notEqual(
    invalid.code,
    0,
    "runtime TCP connection must authenticate, not match a trust rule",
  );
  assert.match(invalid.stderr, /password authentication failed/);
  return { admin, sql, invoke };
}

async function verifyGrants({ sql, invoke }, grants) {
  await sql(
    "darkhorse_owner",
    `ALTER DEFAULT PRIVILEGES GRANT ALL ON TABLES TO darkhorse_runtime;
ALTER DEFAULT PRIVILEGES GRANT SELECT ON TABLES TO PUBLIC;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO darkhorse_runtime;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO PUBLIC;
ALTER DEFAULT PRIVILEGES GRANT ALL ON SEQUENCES TO darkhorse_runtime;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO PUBLIC;
ALTER DEFAULT PRIVILEGES GRANT EXECUTE ON ROUTINES TO darkhorse_runtime;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT EXECUTE ON ROUTINES TO PUBLIC;`,
  );
  await sql("darkhorse_owner", grants);
  await sql(
    "darkhorse_owner",
    `
CREATE TABLE future_operator_state(secret text);
CREATE SEQUENCE future_operator_sequence;
CREATE FUNCTION future_operator_function() RETURNS integer LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog AS 'SELECT 1';
`,
  );
  const checks = await readFile(
    "scripts/tests/integration/database-authority.sql",
    "utf8",
  );
  await sql("darkhorse_runtime", checks);
  await sql(
    "darkhorse_owner",
    `
GRANT SELECT(secret),UPDATE(secret) ON future_operator_state TO darkhorse_runtime;
GRANT SELECT ON future_operator_state TO PUBLIC;
GRANT ALL ON SEQUENCE future_operator_sequence TO darkhorse_runtime;
GRANT UPDATE(event) ON security_audit TO darkhorse_runtime;
GRANT EXECUTE ON FUNCTION future_operator_function() TO PUBLIC;
GRANT USAGE ON SCHEMA darkhorse_migration_v1 TO PUBLIC,darkhorse_runtime,darkhorse_operator;
GRANT SELECT ON darkhorse_migration_v1.intents TO PUBLIC,darkhorse_runtime,darkhorse_operator;
`,
  );
  // Remove old, PUBLIC, column and accidental privileges without automatically
  // exposing a newly added relation or routine. Repeat to prove idempotence.
  for (let repeat = 0; repeat < 2; repeat++) {
    await sql("darkhorse_owner", grants);
    await sql("darkhorse_runtime", checks);
    for (const role of ["darkhorse_runtime", "darkhorse_operator"]) {
      assert.notEqual(
        (await sql(role, "SELECT * FROM darkhorse_migration_v1.intents", true))
          .code,
        0,
      );
    }
  }
  const migrate = await invoke(
    "darkhorse_runtime",
    ["operator", "migrate"],
    true,
  );
  assert.notEqual(
    migrate.code,
    0,
    "runtime login cannot execute the migration command",
  );
}

async function verifyAudit({ sql, invoke }, grants) {
  const principal = "00000000-0000-0000-0000-000000000456";
  await sql(
    "darkhorse_owner",
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${principal}','audit@example.com','Audit','Fixture');
REVOKE INSERT ON security_audit FROM darkhorse_runtime;`,
  );
  // Direct SQL checks the database privilege boundary independently of CLI login.
  const mutation = `BEGIN;
SELECT singleton FROM security_state FOR UPDATE;
UPDATE principals SET revision=revision+1,credential_epoch=credential_epoch+1 WHERE id='${principal}';
INSERT INTO security_audit(event,principal_id,principal_revision,credential_epoch)
 VALUES('account.revoked','${principal}',1,1);
COMMIT;`;
  const failed = await sql("darkhorse_runtime", mutation, true);
  assert.notEqual(failed.code, 0);
  assert.match(failed.stderr, /permission denied/);
  const state = async () =>
    (
      await sql(
        "darkhorse_owner",
        `SELECT revision,credential_epoch,(SELECT count(*) FROM security_audit WHERE principal_id='${principal}' AND database_role='darkhorse_runtime') FROM principals WHERE id='${principal}';`,
      )
    ).stdout.trim();
  assert.equal(await state(), "0|0|0");
  await sql("darkhorse_owner", grants);
  for (const role of ["darkhorse_owner", "darkhorse_runtime"]) {
    const denied = await invoke(
      role,
      ["--output", "json", "operator", "account", "revoke-all", principal, "0"],
      true,
    );
    assert.equal(
      denied.code,
      2,
      "database credentials alone must not invoke an account command",
    );
  }
  await sql("darkhorse_runtime", mutation);
  assert.equal(await state(), "1|1|1");
}

async function verifyTopology({ admin, sql }, grants) {
  for (const [unsafe, restore] of [
    [
      "ALTER ROLE darkhorse_runtime SUPERUSER",
      "ALTER ROLE darkhorse_runtime NOSUPERUSER",
    ],
    [
      "ALTER ROLE darkhorse_runtime CREATEDB",
      "ALTER ROLE darkhorse_runtime NOCREATEDB",
    ],
    [
      "ALTER ROLE darkhorse_runtime CREATEROLE",
      "ALTER ROLE darkhorse_runtime NOCREATEROLE",
    ],
    [
      "ALTER ROLE darkhorse_runtime REPLICATION",
      "ALTER ROLE darkhorse_runtime NOREPLICATION",
    ],
    [
      "ALTER ROLE darkhorse_runtime BYPASSRLS",
      "ALTER ROLE darkhorse_runtime NOBYPASSRLS",
    ],
    [
      "GRANT darkhorse_owner TO darkhorse_runtime WITH INHERIT FALSE, SET TRUE",
      "REVOKE darkhorse_owner FROM darkhorse_runtime",
    ],
    [
      "ALTER TABLE future_operator_state OWNER TO darkhorse_runtime",
      "ALTER TABLE future_operator_state OWNER TO darkhorse_owner",
    ],
    [
      "ALTER DATABASE darkhorse_authority OWNER TO darkhorse_runtime; GRANT CONNECT ON DATABASE darkhorse_authority TO darkhorse_owner",
      "ALTER DATABASE darkhorse_authority OWNER TO darkhorse_owner",
    ],
    [
      "CREATE SCHEMA runtime_owned AUTHORIZATION darkhorse_runtime",
      "DROP SCHEMA runtime_owned",
    ],
    [
      "ALTER FUNCTION future_operator_function() OWNER TO darkhorse_runtime",
      "ALTER FUNCTION future_operator_function() OWNER TO darkhorse_owner",
    ],
  ]) {
    await admin(`${unsafe};`);
    const failed = await sql("darkhorse_owner", grants, true);
    assert.notEqual(failed.code, 0, unsafe);
    assert.match(failed.stderr, /runtime must be a nonowner role/);
    await admin(`${restore};`);
  }
  await sql("darkhorse_owner", grants);
}

async function verifyAtomicGrants({ sql }, grants) {
  await sql(
    "darkhorse_owner",
    `GRANT SELECT ON future_operator_state TO darkhorse_runtime;
ALTER TABLE profile_audit RENAME TO missing_profile_audit;`,
  );
  const failed = await sql("darkhorse_owner", grants, true);
  assert.notEqual(failed.code, 0);
  assert.match(failed.stderr, /relation "profile_audit" does not exist/);
  // An error after the REVOKEs must roll back, never leave a half-applied policy.
  await sql("darkhorse_runtime", "SELECT * FROM future_operator_state;");
  await sql(
    "darkhorse_owner",
    "ALTER TABLE missing_profile_audit RENAME TO profile_audit;",
  );
  await sql("darkhorse_owner", grants);
  const denied = await sql(
    "darkhorse_runtime",
    "SELECT * FROM future_operator_state;",
    true,
  );
  assert.notEqual(denied.code, 0);
  assert.match(denied.stderr, /permission denied/);
}
