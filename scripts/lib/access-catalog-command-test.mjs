// Actual native commands through the selected container launcher.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function accessCatalogCommands(
  invoke,
  sql,
  source,
  password,
  application,
) {
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
COMMIT;`);
  for (const all of [false, true]) {
    const actor = randomUUID(),
      credential = randomUUID(),
      email = `${actor}@example.com`;
    await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Access','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}'); COMMIT;`);
    const input = JSON.stringify({ email, password });
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
      const response = await invoke(
        {
          CATALOG_TARGET: target,
          ...(all
            ? { CATALOG_ALL_DEFINITIONS: "yes", CATALOG_SEARCH: prefix }
            : { CATALOG_APPLICATION_ID: application }),
        },
        input,
      );
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
    await sql(
      `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
    );
    const denied = await invoke(
      {
        CATALOG_TARGET: "role",
        ...(all
          ? { CATALOG_ALL_DEFINITIONS: "yes" }
          : { CATALOG_APPLICATION_ID: application }),
      },
      input,
    );
    accountResult(
      denied,
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
  }
}
