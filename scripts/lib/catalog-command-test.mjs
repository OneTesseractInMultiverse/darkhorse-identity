import { applicationCommands } from "./application-command-test.mjs";
import { clientCommands } from "./client-command-test.mjs";
// Actual container/process evidence; excluded from isolated unit suites.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { accountResult } from "./container-account-test.mjs";
export async function catalogCommands(
  command,
  target,
  settings,
  sql,
  source,
  password,
) {
  const invoke = (selectors, input) =>
    catalogCommand(command, target, { ...settings, ...selectors }, input);
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
INSERT INTO client_redirects(client_id,uri) VALUES('${client}','https://client.example/callback?fixed=1');
COMMIT;`);
  await applicationCommands(invoke, sql, source, password);
  const input = JSON.stringify({ email, password });
  for (const [args, id, fields] of [
    [
      {
        CATALOG_TARGET: "application",
        CATALOG_SEARCH: name,
        CATALOG_STATUS: "active",
        CATALOG_LIMIT: "1",
      },
      app,
      6,
    ],
    [
      {
        CATALOG_TARGET: "client",
        CATALOG_APPLICATION_ID: app,
        CATALOG_LIMIT: "1",
      },
      client,
      5,
    ],
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
  for (const catalogTarget of ["application", "client"]) {
    const selectors = {
      CATALOG_TARGET: catalogTarget,
      CATALOG_OPERATION: "show",
      CATALOG_APPLICATION_ID: app,
      ...(catalogTarget === "client" ? { CATALOG_CLIENT_ID: client } : {}),
    };
    const response = await invoke(selectors, input),
      details = accountResult(response, 0);
    assert.ok(
      !response.stdout.includes(password) &&
        !response.stderr.includes(password),
    );
    assert.equal(details.record.id, catalogTarget === "client" ? client : app);
    assert.equal(details.record.revision, "0");
    assert.equal(details.record.secrets, undefined);
    if (catalogTarget === "client")
      assert.deepEqual(details.record.redirect_uris, [
        "https://client.example/callback?fixed=1",
      ]);
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_catalog_detail_audit WHERE operation_id='${details.operation_id}' AND result='read' AND database_role='darkhorse_runtime';`,
        )
      ).stdout.trim(),
      "1",
    );
  }
  await clientCommands(invoke, sql, source, password, app);
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${actor}';`,
  );
  const denied = await invoke(
    {
      CATALOG_TARGET: "client",
      CATALOG_OPERATION: "show",
      CATALOG_APPLICATION_ID: app,
      CATALOG_CLIENT_ID: client,
    },
    input,
  );
  accountResult(denied, 2, /Administrator authentication or authority denied/);
  assert.ok(!denied.stderr.includes(password));
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_catalog_detail_audit WHERE actor_id='${actor}' AND application_id='${app}' AND client_id='${client}' AND command='client.show' AND result='denied' AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );
}

async function catalogCommand(command, target, settings, input) {
  return command("make", ["--no-print-directory", target], {
    env: {
      ...process.env,
      ACCOUNT_OPERATION: "",
      ACCOUNT_ID: "",
      ACCOUNT_REVISION: "",
      ACCOUNT_CONFIRM: "",
      ACCOUNT_SEARCH: "",
      ACCOUNT_STATUS: "",
      ACCOUNT_AFTER: "",
      ACCOUNT_LIMIT: "",
      CATALOG_TARGET: "",
      CATALOG_OPERATION: "list",
      CATALOG_APPLICATION_ID: "",
      CATALOG_CLIENT_ID: "",
      CATALOG_SEARCH: "",
      CATALOG_STATUS: "",
      CATALOG_AFTER: "",
      CATALOG_LIMIT: "",
      CATALOG_CONFIRM: "",
      CATALOG_REVISION: "",
      CATALOG_NAME: "",
      CATALOG_OWNER_ID: "",
      ...settings,
    },
    input,
    capture: true,
    acceptFailure: true,
  });
}
