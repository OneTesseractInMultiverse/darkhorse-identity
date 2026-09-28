import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";

export async function applicationRoleCommands(
  invoke,
  sql,
  actor,
  password,
  application,
  role,
) {
  const target = randomUUID();
  await sql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','role-target-${target}@example.com','Role','Target');`,
  );
  const policyRevision = async () =>
    (
      await sql(
        "SELECT policy_revision::text FROM security_state WHERE singleton;",
      )
    ).stdout.trim();
  const applyRole = async (assigned, principalRevision) => {
    const expectedPolicyRevision = await policyRevision();
    const response = await invoke(
      {
        CATALOG_TARGET: "access",
        CATALOG_OPERATION: "apply",
        CATALOG_CONFIRM: "yes",
      },
      JSON.stringify({
        authentication: {
          email: `${actor}@example.com`,
          password,
          reason: assigned
            ? "Assign approved application role"
            : "Remove approved application role",
        },
        policy_revision: expectedPolicyRevision,
        change: {
          operation: "principal_role",
          principal_id: target,
          application_id: application,
          role_id: role,
          assigned,
          principal_revision: String(principalRevision),
        },
      }),
    );
    return { expectedPolicyRevision, response };
  };

  const assignment = await applyRole(true, 0);
  const assigned = accountResult(assignment.response, 0);
  assert.equal(assigned.completed, true);
  assert.equal(assigned.changed, true);
  assert.equal(assigned.target.kind, "principal_role");
  assert.equal(assigned.principal_revision, "1");
  assert.ok(
    Number(assigned.policy_revision) >
      Number(assignment.expectedPolicyRevision),
  );
  assert.equal(await policyRevision(), assigned.policy_revision);
  assert.ok(
    !assignment.response.stdout.includes(password) &&
      !assignment.response.stderr.includes(password),
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM principal_roles WHERE principal_id='${target}' AND application_id='${application}' AND role_id='${role}';`,
      )
    ).stdout.trim(),
    "1",
  );
  assert.equal(
    (
      await sql(`SELECT revision::text FROM principals WHERE id='${target}';`)
    ).stdout.trim(),
    assigned.principal_revision,
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM platform_administrators WHERE principal_id='${target}';`,
      )
    ).stdout.trim(),
    "0",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_access_catalog_audit WHERE operation_id='${assigned.operation_id}' AND actor_id='${actor}' AND command='principal.role' AND target_id='${target}' AND application_id='${application}' AND related_id='${role}' AND expected_revision=${assignment.expectedPolicyRevision} AND resulting_revision=${assigned.policy_revision} AND principal_expected_revision=0 AND principal_resulting_revision=1 AND requested_state AND result='changed' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );

  const stale = await applyRole(false, 0);
  accountResult(
    stale.response,
    2,
    /The policy changed; read its current revision before retrying\./,
  );
  assert.equal(await policyRevision(), stale.expectedPolicyRevision);
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM principal_roles WHERE principal_id='${target}' AND application_id='${application}' AND role_id='${role}';`,
      )
    ).stdout.trim(),
    "1",
  );
  assert.equal(
    (
      await sql(`SELECT revision::text FROM principals WHERE id='${target}';`)
    ).stdout.trim(),
    "1",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_access_catalog_audit WHERE target_id='${target}' AND application_id='${application}' AND related_id='${role}' AND command='principal.role' AND expected_revision=${stale.expectedPolicyRevision} AND principal_expected_revision=0 AND principal_resulting_revision IS NULL AND result='conflict' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );

  const removal = await applyRole(false, 1);
  const removed = accountResult(removal.response, 0);
  assert.equal(removed.changed, true);
  assert.equal(removed.principal_revision, "2");
  assert.ok(
    Number(removed.policy_revision) > Number(removal.expectedPolicyRevision),
  );
  assert.equal(await policyRevision(), removed.policy_revision);
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM principal_roles WHERE principal_id='${target}' AND application_id='${application}' AND role_id='${role}';`,
      )
    ).stdout.trim(),
    "0",
  );
  assert.equal(
    (
      await sql(`SELECT revision::text FROM principals WHERE id='${target}';`)
    ).stdout.trim(),
    removed.principal_revision,
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM platform_administrators WHERE principal_id='${target}';`,
      )
    ).stdout.trim(),
    "0",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_access_catalog_audit WHERE operation_id='${removed.operation_id}' AND actor_id='${actor}' AND command='principal.role' AND target_id='${target}' AND application_id='${application}' AND related_id='${role}' AND expected_revision=${removal.expectedPolicyRevision} AND resulting_revision=${removed.policy_revision} AND principal_expected_revision=1 AND principal_resulting_revision=2 AND NOT requested_state AND result='changed' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );
}
