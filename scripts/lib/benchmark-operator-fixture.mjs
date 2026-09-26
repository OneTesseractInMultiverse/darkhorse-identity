// Disposable performance fixture only; all timed commands use normal authentication.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { operatorResult, operatorRead } from "./benchmark-operator-model.mjs";

async function scalar({ docker, db }, sql) {
  return (
    await docker([
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
    ])
  ).stdout.trim();
}
async function actor(options) {
  const principal = randomUUID(),
    credential = randomUUID(),
    email = `${principal}@example.com`;
  await options.runSql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${principal}','${email}','Benchmark','Operator');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${principal}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${options.principal}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${principal}'); COMMIT;`);
  return { principal, email };
}
async function invoke(options, actor, args, reason) {
  const input = JSON.stringify({
    email: actor.email,
    password: options.password,
    ...(reason ? { reason } : {}),
  });
  const stdout = await options.benchmarkInvoke(
    [
      "--auth-stdin",
      "--output",
      "json",
      ...(reason ? ["--yes"] : []),
      "operator",
      ...args,
    ],
    input,
  );
  assert.ok(
    !stdout.includes(options.password),
    "Benchmark operator exposed protected input.",
  );
  return operatorResult(stdout);
}
async function verifyReads(options, actors, commands) {
  for (const row of commands) {
    const { table } = operatorRead(row.operation, {});
    const populated = row.operation.endsWith(".list")
      ? " AND returned_count>0"
      : "";
    assert.equal(
      await scalar(
        options,
        `SELECT count(*) FROM ${table} WHERE operation_id='${row.operationId}' AND actor_id='${actors[row.worker].principal}' AND command='${row.operation}' AND result='read' AND database_role='${options.databaseRole}'${populated}`,
      ),
      "1",
      "Missing successful operator read audit.",
    );
  }
}
async function population(options, ids, details) {
  if (!details) return { additionalPrincipals: 0, additionalBoundRoles: 0 };
  await options.runSql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name)
SELECT gen_random_uuid(),'population-'||n||'@example.com','Population','Fixture' FROM generate_series(1,1000) n;
WITH added AS (INSERT INTO roles(id,name) SELECT gen_random_uuid(),'Population role '||n FROM generate_series(1,64) n RETURNING id)
INSERT INTO role_applications SELECT '${ids.application}',id FROM added;
INSERT INTO role_capabilities SELECT ra.role_id,c.capability_id FROM role_applications ra CROSS JOIN capability_applications c JOIN roles r ON r.id=ra.role_id
WHERE ra.application_id='${ids.application}' AND c.application_id='${ids.application}' AND r.name LIKE 'Population role %';
COMMIT;`);
  return { additionalPrincipals: 1000, additionalBoundRoles: 64 };
}
export async function operatorFixture(options, app, details = false) {
  assert.match(
    options.principal,
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
  );
  assert.ok(["postgres", "darkhorse_runtime"].includes(options.databaseRole));
  const ids = {
    principal: options.principal,
    application: app.identity.application_id,
    client: app.client,
  };
  for (const id of Object.values(ids))
    assert.match(
      id,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
  const populationSize = await population(options, ids, details);
  const actors = [await actor(options), await actor(options)];
  const revision = await scalar(
    options,
    `SELECT revision FROM principals WHERE id='${options.principal}'`,
  );
  assert.match(revision, /^(0|[1-9][0-9]{0,18})$/);
  let revocation;
  return {
    population: populationSize,
    read: (worker, operation) =>
      invoke(options, actors[worker], operatorRead(operation, ids).args),
    revoke: async () => {
      revocation = await invoke(
        options,
        { email: "browser@example.com" },
        ["account", "revoke-all", options.principal, revision],
        "Performance fixture revocation",
      );
    },
    verify: async (commands) => {
      await verifyReads(options, actors, commands);
      assert.equal(
        await scalar(
          options,
          `SELECT count(*) FROM operator_account_audit WHERE operation_id='${revocation.operationId}' AND actor_id='${options.principal}' AND target_id='${options.principal}' AND command='account.revoke_all' AND expected_revision=${revision} AND result='changed' AND database_role='${options.databaseRole}'`,
        ),
        "1",
        "Missing committed operator revocation audit.",
      );
      return { readCommands: commands.length, revocationCommands: 1 };
    },
  };
}
