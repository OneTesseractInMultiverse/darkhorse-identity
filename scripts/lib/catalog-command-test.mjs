// Actual container/process evidence; excluded from isolated unit suites.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function catalogCommands(invoke, sql, source, password) {
  const actor = randomUUID(),
    credential = randomUUID(),
    app = randomUUID(),
    client = randomUUID();
  const email = `${actor}@example.com`,
    name = `Catalog ${app}`;
  await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Catalog','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}');
INSERT INTO applications(id,name,owner_id,active) VALUES('${app}','${name}','${actor}',true);
INSERT INTO oauth_clients(id,application_id,name,active) VALUES('${client}','${app}','Catalog client',true);
COMMIT;`);
  const input = JSON.stringify({ email, password });
  for (const [args, id, fields] of [
    [
      ["operator", "application", "list", "--search", name, "--limit", "1"],
      app,
      6,
    ],
    [["operator", "client", "list", app, "--limit", "1"], client, 5],
  ]) {
    const result = await invoke(args, input),
      page = accountResult(result, 0);
    assert.ok(
      !result.stdout.includes(password) && !result.stderr.includes(password),
    );
    assert.equal(page.items.length, 1);
    assert.equal(page.items[0].id, id);
    assert.equal(page.next, null);
    assert.equal(Object.keys(page.items[0]).length, fields);
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_catalog_audit WHERE operation_id='${page.operation_id}' AND result='read' AND returned_count=1 AND database_role='darkhorse_runtime';`,
        )
      ).stdout.trim(),
      "1",
    );
  }
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
  );
  const denied = await invoke(["operator", "client", "list", app], input);
  accountResult(denied, 1, /Administrator authentication or authority denied/);
  assert.ok(!denied.stderr.includes(password));
}
