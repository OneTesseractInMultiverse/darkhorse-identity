// Real password-authenticated SQL and CLI evidence, separate from unit coverage.
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";

export async function verifyOperatorAuthority(fixture, grants) {
  const { sql, invoke } = fixture;
  const checks = await readFile(
    "scripts/tests/integration/operator-authority.sql",
    "utf8",
  );
  await sql("darkhorse_operator", checks);
  const invalid = await sql(
    "darkhorse_operator",
    "SELECT 1;",
    true,
    randomBytes(32).toString("hex"),
  );
  assert.notEqual(invalid.code, 0);
  assert.match(invalid.stderr, /password authentication failed/);
  assert.notEqual(
    (await invoke("darkhorse_operator", ["operator", "migrate"], true)).code,
    0,
  );
  await verifyCommands(fixture, grants);
  await verifyActivationJournal(fixture);
  await verifyReapplication(fixture, grants, checks);
  console.log(
    "Operator bootstrap/signing/limiter commands, audit rollback, SQL denials, unsafe-role refusal and migration separation passed.",
  );
}

async function verifyCommands({ sql, invoke }, grants) {
  const operator = (args, fail = false, input = "") =>
    invoke("darkhorse_operator", args, fail, {
      input,
      env: {
        DARKHORSE_PUBLIC_ORIGIN: "https://authority.example.com",
        DARKHORSE_PROVIDER_ENABLED: "true",
        DARKHORSE_SIGNING_WRAP_KEY: "a".repeat(64),
      },
    });
  const boot = JSON.stringify({
    email: "operator@example.com",
    first_name: "Operator",
    last_name: "Fixture",
    password: randomBytes(24).toString("base64url"),
  });
  await sql(
    "darkhorse_owner",
    "REVOKE INSERT ON security_audit FROM darkhorse_operator;",
  );
  assert.notEqual(
    (await operator(["operator", "bootstrap", "--stdin"], true, boot)).code,
    0,
  );
  assert.equal(
    (
      await sql(
        "darkhorse_owner",
        "SELECT bootstrapped,(SELECT count(*) FROM principals WHERE email='operator@example.com') FROM security_state;",
      )
    ).stdout.trim(),
    "f|0",
  );
  await sql("darkhorse_owner", grants);
  await operator(["operator", "bootstrap", "--stdin"], false, boot);
  assert.notEqual(
    (await operator(["operator", "bootstrap", "--stdin"], true, boot)).code,
    0,
  );
  assert.equal(
    (
      await sql(
        "darkhorse_owner",
        "SELECT database_role FROM security_audit WHERE event='bootstrap';",
      )
    ).stdout.trim(),
    "darkhorse_operator",
  );
  await operator(["signing-status"]);
  await sql(
    "darkhorse_owner",
    "REVOKE INSERT ON provider_audit FROM darkhorse_operator;",
  );
  assert.notEqual((await operator(["signing-generate", "0"], true)).code, 0);
  assert.equal(
    (
      await sql(
        "darkhorse_owner",
        "SELECT revision,(SELECT count(*) FROM signing_keys) FROM provider_state;",
      )
    ).stdout.trim(),
    "0|0",
  );
  await sql("darkhorse_owner", grants);
  const staged = JSON.parse((await operator(["signing-generate", "0"])).stdout);
  assert.equal(staged.revision, 1);
  assert.notEqual(
    (await operator(["signing-activate", staged.kid, "1"], true)).code,
    0,
  );
  await sql(
    "darkhorse_owner",
    "REVOKE INSERT ON limiter_audit FROM darkhorse_operator;",
  );
  assert.notEqual((await operator(["limiter-fence"], true)).code, 0);
  assert.equal(
    (
      await sql("darkhorse_owner", "SELECT count(*) FROM limiter_authority;")
    ).stdout.trim(),
    "0",
  );
  await sql("darkhorse_owner", grants);
  const fenced = JSON.parse((await operator(["limiter-fence"])).stdout);
  assert.equal(fenced.phase, "cooling");
  assert.ok(fenced.not_before_ms - fenced.database_ms >= 903000);
  await operator(["limiter-status"]);
  assert.notEqual((await operator(["limiter-activate"], true)).code, 0);
  assert.notEqual(
    (
      await operator(
        [
          "--auth-stdin",
          "operator",
          "account",
          "show",
          "00000000-0000-0000-0000-000000000456",
        ],
        true,
      )
    ).code,
    0,
  );
}

async function verifyReapplication({ sql, admin }, grants, checks) {
  await sql(
    "darkhorse_owner",
    `GRANT SELECT(secret),UPDATE(secret) ON future_operator_state TO darkhorse_operator;
GRANT UPDATE(event) ON provider_audit TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES GRANT ALL ON TABLES TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES GRANT ALL ON SEQUENCES TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES GRANT EXECUTE ON ROUTINES TO darkhorse_operator;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT EXECUTE ON ROUTINES TO darkhorse_operator;
ALTER TABLE profile_audit RENAME TO missing_profile_audit;`,
  );
  assert.notEqual((await sql("darkhorse_owner", grants, true)).code, 0);
  await sql("darkhorse_operator", "SELECT secret FROM future_operator_state;");
  await sql(
    "darkhorse_owner",
    "ALTER TABLE missing_profile_audit RENAME TO profile_audit;",
  );
  for (let n = 0; n < 2; n++) {
    await sql("darkhorse_owner", grants);
    await sql("darkhorse_operator", checks);
  }
  await sql(
    "darkhorse_owner",
    "CREATE TABLE later_operator_table(value text); CREATE SEQUENCE later_operator_sequence; CREATE FUNCTION later_operator_function() RETURNS integer LANGUAGE sql AS 'SELECT 1';",
  );
  for (const query of [
    "SELECT * FROM later_operator_table;",
    "SELECT nextval('later_operator_sequence');",
    "SELECT later_operator_function();",
  ])
    assert.notEqual((await sql("darkhorse_operator", query, true)).code, 0);
  const cases = [
    [
      "ALTER ROLE darkhorse_operator RENAME TO missing_operator",
      "ALTER ROLE missing_operator RENAME TO darkhorse_operator",
    ],
    ...["SUPERUSER", "CREATEDB", "CREATEROLE", "REPLICATION", "BYPASSRLS"].map(
      (v) => [
        `ALTER ROLE darkhorse_operator ${v}`,
        `ALTER ROLE darkhorse_operator NO${v}`,
      ],
    ),
    [
      "GRANT darkhorse_owner TO darkhorse_operator WITH INHERIT FALSE, SET TRUE",
      "REVOKE darkhorse_owner FROM darkhorse_operator",
    ],
    [
      "ALTER TABLE future_operator_state OWNER TO darkhorse_operator",
      "ALTER TABLE future_operator_state OWNER TO darkhorse_owner",
    ],
    [
      "CREATE SCHEMA unsafe_operator AUTHORIZATION darkhorse_operator",
      "DROP SCHEMA unsafe_operator",
    ],
    [
      "ALTER FUNCTION future_operator_function() OWNER TO darkhorse_operator",
      "ALTER FUNCTION future_operator_function() OWNER TO darkhorse_owner",
    ],
    [
      "ALTER DATABASE darkhorse_authority OWNER TO darkhorse_operator; GRANT CONNECT ON DATABASE darkhorse_authority TO darkhorse_owner",
      "ALTER DATABASE darkhorse_authority OWNER TO darkhorse_owner",
    ],
  ];
  for (const [alter, restore] of cases) {
    await admin(alter);
    const result = await sql("darkhorse_owner", grants, true);
    assert.notEqual(result.code, 0);
    assert.match(result.stderr, /operator must be a nonowner role/);
    await admin(restore);
  }
  await sql("darkhorse_owner", grants);
}

async function verifyActivationJournal({ sql, invoke }) {
  const id = "00000000-0000-0000-0000-000000000789";
  await sql(
    "darkhorse_operator",
    `INSERT INTO limiter_activation_intents(operation_id,epoch,generation,not_before_ms) SELECT '${id}',epoch,generation,0 FROM limiter_authority;`,
  );
  const inspect = ["--output", "json", "operator", "limiter", "inspect", id];
  const record = JSON.parse(
    (await invoke("darkhorse_operator", inspect)).stdout,
  ).data;
  assert.equal(record.recorded_outcome, "pending");
  assert.equal(record.database_role, "darkhorse_operator");
  assert.equal(record.current_phase, "cooling");
  assert.ok(record.prepared_ms <= record.database_ms);
  assert.notEqual((await invoke("darkhorse_runtime", inspect, true)).code, 0);
  // Receipts require activation and cannot impersonate another database login.
  const receipt = `INSERT INTO limiter_activation_receipts(operation_id,run_id,replication_id) VALUES('${id}',decode(repeat('ab',20),'hex'),decode(repeat('cd',20),'hex'));`;
  assert.notEqual((await sql("darkhorse_owner", receipt, true)).code, 0);
  assert.notEqual((await sql("darkhorse_operator", receipt, true)).code, 0);
  // Advance only this owner-controlled disposable fixture past its recovery wait.
  await sql(
    "darkhorse_owner",
    "ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;",
  );
  await sql(
    "darkhorse_operator",
    "UPDATE limiter_authority SET active=true,run_id=decode(repeat('ab',20),'hex'),replication_id=decode(repeat('cd',20),'hex');",
  );
  assert.notEqual((await sql("darkhorse_owner", receipt, true)).code, 0);
  await sql("darkhorse_operator", receipt);
  const completed = JSON.parse(
    (await invoke("darkhorse_operator", inspect)).stdout,
  ).data;
  assert.equal(completed.recorded_outcome, "activated");
  assert.equal(completed.current_phase, "active");
  assert.ok(completed.completed_ms >= record.prepared_ms);
  assert.notEqual((await sql("darkhorse_operator", receipt, true)).code, 0);
}
