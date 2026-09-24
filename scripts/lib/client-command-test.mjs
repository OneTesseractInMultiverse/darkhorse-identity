// Real container mutation checks exercise complete configuration over stdin.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function clientCommands(invoke, sql, source, password, app) {
  const actor = randomUUID(),
    credential = randomUUID(),
    client = randomUUID();
  const email = `${actor}@example.com`;
  await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${actor}','${email}','Client','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${actor}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${actor}');
INSERT INTO oauth_clients(id,application_id,name,active) VALUES('${client}','${app}','Mutable client',true);
INSERT INTO client_redirects(client_id,uri) VALUES('${client}','https://client.example/old');
COMMIT;`);
  const input = JSON.stringify({
    authentication: { email, password, reason: "Verify client configuration" },
    client: {
      name: "Updated client",
      active: false,
      refresh_tokens: true,
      redirect_uris: ["https://client.example/new?fixed=%2F"],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  const selectors = {
    CATALOG_TARGET: "client",
    CATALOG_OPERATION: "update",
    CATALOG_APPLICATION_ID: app,
    CATALOG_CLIENT_ID: client,
    CATALOG_REVISION: "0",
    CATALOG_CONFIRM: "yes",
  };
  const response = await invoke(selectors, input);
  const changed = accountResult(response, 0);
  assert.equal(Object.keys(changed).length, 5);
  assert.equal(changed.application_id, app);
  assert.equal(changed.client_id, client);
  assert.equal(changed.revision, "1");
  for (const hidden of [
    password,
    email,
    "https://client.example",
    "Updated client",
  ])
    assert.ok(
      !response.stdout.includes(hidden) && !response.stderr.includes(hidden),
    );
  accountResult(await invoke(selectors, input), 2, /changed/);
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
  );
  accountResult(
    await invoke({ ...selectors, CATALOG_REVISION: "1" }, input),
    2,
    /Administrator authentication or authority denied/,
  );
  assert.equal(
    (
      await sql(
        `SELECT active::text||'|'||refresh_tokens::text||'|'||revision::text FROM oauth_clients WHERE id='${client}';`,
      )
    ).stdout.trim(),
    "false|true|1",
  );
  assert.equal(
    (
      await sql(`SELECT uri FROM client_redirects WHERE client_id='${client}';`)
    ).stdout.trim(),
    "https://client.example/new?fixed=%2F",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_client_audit WHERE actor_id='${actor}' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "3",
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
