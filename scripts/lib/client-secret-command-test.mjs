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
  const rotate = {
    ...base,
    CATALOG_OPERATION: "rotate",
    CATALOG_REVISION: "0",
    CATALOG_OVERLAP_SECONDS: "30",
    CATALOG_CONFIRM: "yes",
    CATALOG_SECRET_STDOUT: "yes",
  };
  const rotatedResponse = await invoke(
    rotate,
    JSON.stringify({ email, password, reason: "Rotate fixture credential" }),
  );
  const rotated = accountResult(rotatedResponse, 0);
  assert.equal(rotated.revision, "1");
  assert.equal(rotated.overlap_seconds, 30);
  assert.equal(rotated.client_secret.length, 64);
  assert.notEqual(rotated.secret_id, secret);
  for (const hidden of [password, email, verifier])
    assert.ok(
      !rotatedResponse.stderr.includes(hidden),
      "operator credentials and stored verifier stay out of diagnostics",
    );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM oauth_client_secrets WHERE id='${secret}' AND expires_ms IS NOT NULL AND NOT retired;`,
      )
    ).stdout.trim(),
    "1",
    "the previous secret honors the explicit overlap window",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_client_secret_rotation_audit WHERE client_id='${client}' AND result='written' AND overlap_seconds=30 AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );
  const rotationAudit = (
    await sql(
      `SELECT row_to_json(a)::text FROM operator_client_secret_rotation_audit a WHERE client_id='${client}' AND reason LIKE '%credential%' AND result='written';`,
    )
  ).stdout.trim();
  assert.ok(rotationAudit);
  for (const hidden of [password, email, verifier, rotated.client_secret])
    assert.ok(!rotationAudit.includes(hidden));
  const retire = {
    ...base,
    CATALOG_OPERATION: "retire",
    CATALOG_SECRET_ID: secret,
    CATALOG_REVISION: "1",
    CATALOG_CONFIRM: "yes",
  };
  const response = await invoke(retire, mutation);
  const changed = accountResult(response, 0);
  assert.equal(changed.revision, "2");
  assert.equal(changed.secret_id, secret);
  assert.equal(changed.completed, true);
  for (const hidden of [password, email, verifier])
    assert.ok(
      !response.stdout.includes(hidden) && !response.stderr.includes(hidden),
    );
  accountResult(await invoke(retire, mutation), 2, /changed/);
  const current = JSON.parse(
    (
      await sql(
        `SELECT coalesce(json_agg(json_build_object('id',id::text,'retired',retired) ORDER BY id)::text,'[]') FROM oauth_client_secrets WHERE client_id='${client}';`,
      )
    ).stdout.trim(),
  );
  assert.equal(current.length, 2);
  assert.equal(current.find((item) => item.id === secret).retired, true);
  assert.equal(
    current.find((item) => item.id === rotated.secret_id).retired,
    false,
  );
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
