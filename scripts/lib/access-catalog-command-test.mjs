// Actual native commands through the selected container launcher.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
import { applicationRoleCommands } from "./application-role-command-test.mjs";
export async function accessCatalogCommands(
  invoke,
  sql,
  source,
  password,
  application,
  exerciseComposeRoleAssignment = false,
) {
  let actor;
  let accountAttempts = 0;
  const rotateActor = async () => {
    actor = randomUUID();
    const credential = randomUUID(),
      email = `${actor}@example.com`;
    await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Access','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}'); COMMIT;`);
    accountAttempts = 0;
    return actor;
  };
  const invokeWithinBudget = async (...args) => {
    if (accountAttempts === 5) {
      await rotateActor();
    }
    accountAttempts++;
    const input = JSON.stringify({
      email: `${actor}@example.com`,
      password,
    });
    return invoke(args[0], input);
  };
  const resource = randomUUID(),
    scope = randomUUID(),
    role = randomUUID(),
    capability = randomUUID(),
    unboundRole = randomUUID(),
    unboundCapability = randomUUID();
  const prefix = `access-${resource}`;
  await sql(`BEGIN;
INSERT INTO protected_resources(id,application_id,name,audience) VALUES('${resource}','${application}','Access resource','urn:darkhorse:resource:${resource}');
INSERT INTO resource_scopes(id,application_id,resource_id,name) VALUES('${scope}','${application}','${resource}','access:read');
INSERT INTO roles(id,name) VALUES('${role}','${prefix}-bound'),('${unboundRole}','${prefix}-unbound');
INSERT INTO capabilities(id,permission_key,meaning) VALUES('${capability}','${prefix}-bound','private-description-marker'),('${unboundCapability}','${prefix}-unbound','private-description-marker');
INSERT INTO role_applications VALUES('${application}','${role}');
INSERT INTO capability_applications VALUES('${application}','${capability}');
INSERT INTO role_capabilities VALUES('${role}','${capability}');
INSERT INTO resource_capabilities VALUES('${application}','${resource}','${capability}');
INSERT INTO scope_capabilities VALUES('${application}','${resource}','${scope}','${capability}');
  COMMIT;`);
  for (const all of [false, true]) {
    await rotateActor();
    const cases = all
      ? [
          ["role", role, 2],
          ["capability", capability, 3],
        ]
      : [
          ["resource", resource, 4],
          ["scope", scope, 4],
          ["role", role, 2],
          ["capability", capability, 3],
        ];
    for (const [target, id, fields] of cases) {
      const response = await invokeWithinBudget({
        CATALOG_TARGET: target,
        ...(all
          ? { CATALOG_ALL_DEFINITIONS: "yes", CATALOG_SEARCH: prefix }
          : { CATALOG_APPLICATION_ID: application }),
      });
      const page = accountResult(response, 0);
      assert.equal(page.items.length, all ? 2 : 1);
      assert.ok(page.items.some((item) => item.id === id));
      assert.equal(Object.keys(page.items[0]).length, fields);
      assert.equal(page.next, null);
      assert.ok(
        !response.stdout.includes(password) &&
          !response.stderr.includes(password) &&
          !response.stdout.includes("private-description-marker"),
      );
      assert.equal(
        (
          await sql(
            `SELECT count(*) FROM operator_catalog_audit WHERE operation_id='${page.operation_id}' AND command='${target}.list' AND ${all ? "application_id IS NULL" : `application_id='${application}'`} AND result='read' AND database_role='darkhorse_runtime';`,
          )
        ).stdout.trim(),
        "1",
      );
    }
    const detailCases = all
      ? [
          ["role", role, null, 1, 1],
          ["capability", capability, null, 1, 0],
        ]
      : [
          ["resource", resource, null, 0, 1],
          ["scope", scope, resource, 0, 1],
          ["role", role, null, 1, 1],
          ["capability", capability, null, 1, 0],
        ];
    for (const [
      target,
      targetId,
      parentResource,
      appCount,
      capabilityCount,
    ] of detailCases) {
      const details = accountResult(
        await invokeWithinBudget({
          CATALOG_TARGET: target,
          CATALOG_OPERATION: "show",
          CATALOG_TARGET_ID: targetId,
          ...(parentResource ? { CATALOG_RESOURCE_ID: parentResource } : {}),
          ...(all
            ? { CATALOG_ALL_DEFINITIONS: "yes" }
            : { CATALOG_APPLICATION_ID: application }),
        }),
        0,
      );
      assert.equal(details.record.item.id, targetId);
      assert.equal(details.record.applications.length, appCount);
      assert.equal(details.record.capabilities.length, capabilityCount);
      assert.equal(
        details.record.definition_scope,
        all ? "all-definitions" : "application",
      );
      assert.ok(!JSON.stringify(details).includes(password));
      assert.ok(!JSON.stringify(details).includes("private-owner@example.com"));
      if (target === "capability")
        assert.equal(details.record.item.meaning, "private-description-marker");
      else
        assert.ok(
          !JSON.stringify(details).includes("private-description-marker"),
        );
      assert.equal(
        (
          await sql(
            `SELECT count(*) FROM operator_access_detail_audit WHERE operation_id='${details.operation_id}' AND command='${target}.show' AND target_id='${targetId}' AND ${all ? "application_id IS NULL" : `application_id='${application}'`} AND result='read' AND database_role='darkhorse_runtime';`,
          )
        ).stdout.trim(),
        "1",
      );
    }
    if (!all) {
      const absent = accountResult(
        await invokeWithinBudget({
          CATALOG_TARGET: "role",
          CATALOG_OPERATION: "show",
          CATALOG_APPLICATION_ID: application,
          CATALOG_TARGET_ID: unboundRole,
        }),
        2,
        /Catalog target not found/,
      );
      assert.equal(absent, undefined);
      assert.equal(
        (
          await sql(
            `SELECT count(*) FROM operator_access_detail_audit WHERE command='role.show' AND application_id='${application}' AND target_id='${unboundRole}' AND result='not_found' AND database_role='darkhorse_runtime';`,
          )
        ).stdout.trim(),
        "1",
      );
      const before = (
        await sql("SELECT count(*) FROM operator_access_detail_audit;")
      ).stdout.trim();
      await sql(
        "REVOKE INSERT ON operator_access_detail_audit FROM darkhorse_runtime;",
      );
      accountResult(
        await invokeWithinBudget({
          CATALOG_TARGET: "resource",
          CATALOG_OPERATION: "show",
          CATALOG_APPLICATION_ID: application,
          CATALOG_TARGET_ID: resource,
        }),
        2,
        /Catalog read unavailable/,
      );
      await sql(
        "GRANT SELECT,INSERT ON operator_access_detail_audit TO darkhorse_runtime;",
      );
      assert.equal(
        (
          await sql("SELECT count(*) FROM operator_access_detail_audit;")
        ).stdout.trim(),
        before,
      );
    }
    await rotateActor();
    await sql(
      `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
    );
    const denied = await invokeWithinBudget({
      CATALOG_TARGET: "role",
      ...(all
        ? { CATALOG_ALL_DEFINITIONS: "yes" }
        : { CATALOG_APPLICATION_ID: application }),
    });
    accountResult(
      denied,
      2,
      /Administrator authentication or authority denied/,
    );
    accountResult(
      await invokeWithinBudget({
        CATALOG_TARGET: "role",
        CATALOG_OPERATION: "show",
        CATALOG_ALL_DEFINITIONS: "yes",
        CATALOG_TARGET_ID: role,
      }),
      2,
      /Administrator authentication or authority denied/,
    );
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_catalog_audit WHERE actor_id='${actor}' AND command='role.list' AND result='denied' AND database_role='darkhorse_runtime';`,
        )
      ).stdout.trim(),
      "1",
    );
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_access_detail_audit WHERE actor_id='${actor}' AND command='role.show' AND target_id='${role}' AND result='denied' AND database_role='darkhorse_runtime';`,
        )
      ).stdout.trim(),
      "1",
    );
  }

  if (exerciseComposeRoleAssignment) {
    const roleOperator = await rotateActor();
    await applicationRoleCommands(
      invoke,
      sql,
      roleOperator,
      password,
      application,
      role,
    );
  }
}
