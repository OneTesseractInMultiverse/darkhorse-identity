// Real container mutation checks share the catalog's protected-stdin launcher.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function applicationCommands(invoke, sql, source, password) {
  const actor = randomUUID(),
    credential = randomUUID(),
    email = `${actor}@example.com`;
  await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Application','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}'); COMMIT;`);
  const input = JSON.stringify({
    email,
    password,
    reason: "Verify application administration",
  });
  const spec = {
    CATALOG_TARGET: "application",
    CATALOG_NAME: `Application ${actor}`,
    CATALOG_OWNER_ID: source,
    CATALOG_STATUS: "active",
    CATALOG_CONFIRM: "yes",
  };
  const response = await invoke(
    { ...spec, CATALOG_OPERATION: "create" },
    input,
  );
  const created = accountResult(response, 0);
  assert.equal(created.revision, "0");
  assert.equal(Object.keys(created).length, 4);
  assert.ok(
    !response.stdout.includes(password) && !response.stderr.includes(password),
  );
  const update = {
    ...spec,
    CATALOG_OPERATION: "update",
    CATALOG_APPLICATION_ID: created.application_id,
    CATALOG_REVISION: "0",
    CATALOG_OWNER_ID: actor,
    CATALOG_STATUS: "inactive",
  };
  const changed = accountResult(await invoke(update, input), 0);
  assert.equal(changed.revision, "1");
  assert.equal(changed.application_id, created.application_id);
  accountResult(await invoke(update, input), 2, /changed/);
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
  );
  accountResult(
    await invoke({ ...update, CATALOG_REVISION: "1" }, input),
    2,
    /Administrator authentication or authority denied/,
  );
  assert.equal(
    (
      await sql(
        `SELECT owner_id::text||'|'||active::text||'|'||revision::text FROM applications WHERE id='${created.application_id}';`,
      )
    ).stdout.trim(),
    `${actor}|false|1`,
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_application_audit WHERE actor_id='${actor}' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "4",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM registration_audit WHERE actor_id='${actor}';`,
      )
    ).stdout.trim(),
    "2",
  );
}
