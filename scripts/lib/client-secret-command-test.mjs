// Lifecycle checks against actual container commands and restricted runtime roles.
import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function clientSecretCommands(invoke, sql, source, password, app) {
  const actor = randomUUID(),
    credential = randomUUID(),
    client = randomUUID(),
    secret = randomUUID();
  const email = `${actor}@example.com`,
    verifier = randomBytes(32).toString("hex");
  await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Secret','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}');
INSERT INTO oauth_clients(id,application_id,name,active) VALUES('${client}','${app}','Secret fixture',true);
INSERT INTO client_redirects(client_id,uri) VALUES('${client}','https://client.example/callback');
INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('${secret}','${client}',decode('${verifier}','hex'),0);
COMMIT;`);
  const base = {
    CATALOG_TARGET: "client-secret",
    CATALOG_APPLICATION_ID: app,
    CATALOG_CLIENT_ID: client,
  };
  const read = JSON.stringify({ email, password });
  const mutation = JSON.stringify({
    email,
    password,
    reason: "Retire fixture credential",
  });
  const first = accountResult(
    await invoke({ ...base, CATALOG_LIMIT: "1" }, read),
    0,
  );
  assert.equal(first.revision, "0");
  assert.deepEqual(first.items, [
    { id: secret, created_ms: 0, expires_ms: null, retired: false },
  ]);
  assert.equal(first.next, null);
  const retire = {
    ...base,
    CATALOG_OPERATION: "retire",
    CATALOG_SECRET_ID: secret,
    CATALOG_REVISION: "0",
    CATALOG_CONFIRM: "yes",
  };
  const response = await invoke(retire, mutation);
  const changed = accountResult(response, 0);
  assert.equal(changed.revision, "1");
  assert.equal(changed.secret_id, secret);
  assert.equal(changed.completed, true);
  for (const hidden of [password, email, verifier])
    assert.ok(
      !response.stdout.includes(hidden) && !response.stderr.includes(hidden),
    );
  accountResult(await invoke(retire, mutation), 2, /changed/);
  const current = accountResult(await invoke(base, read), 0);
  assert.equal(current.revision, "1");
  assert.equal(current.items[0].retired, true);
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
  );
  accountResult(
    await invoke(base, read),
    2,
    /Administrator authentication or authority denied/,
  );
  assert.equal(
    (
      await sql(
        `SELECT retired::text FROM oauth_client_secrets WHERE id='${secret}';`,
      )
    ).stdout.trim(),
    "true",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_client_secret_audit WHERE actor_id='${actor}' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "5",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM registration_audit WHERE actor_id='${actor}';`,
      )
    ).stdout.trim(),
    "1",
  );
}
