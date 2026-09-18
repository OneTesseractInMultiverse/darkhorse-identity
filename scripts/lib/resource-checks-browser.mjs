import assert from "node:assert/strict";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import {
  exchange,
  manage,
  userinfo,
  validateCallback,
  validateIdToken,
} from "./reference-client.mjs";

async function register({ page, call, principal, origin, runSql }, name) {
  const application = await call(page, "/api/admin/registration", {
    operation: "create_application",
    application: { name, owner_id: principal, active: true },
  });
  assert.equal(application.status, 200);
  const applicationId = application.body.record.id;
  const resource = await call(page, "/api/admin/registration", {
    operation: "create_resource",
    application_id: applicationId,
    name: `${name} API`,
  });
  assert.equal(resource.status, 200);
  const resourceId = resource.body.record.id;
  const scope = await call(page, "/api/admin/registration", {
    operation: "create_scope",
    application_id: applicationId,
    resource_id: resourceId,
    name: "operate",
  });
  assert.equal(scope.status, 200);
  const scopeId = scope.body.record.id;
  const client = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: applicationId,
    client: {
      name,
      active: true,
      redirect_uris: [`${origin}/callback?resource=${resourceId}`],
      resource_ids: [resourceId],
      scope_ids: [scopeId],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(client.status, 200);
  const identity = { application_id: applicationId, resource_id: resourceId };
  const credential = await call(page, "/api/admin/resource-introspection", {
    operation: "register",
    ...identity,
  });
  assert.equal(credential.status, 200);
  assert.match(credential.body.secret, /^[a-f0-9]{64}$/);
  assert.equal(credential.body.introspection_client_id, `rs_${resourceId}`);
  const metadata = await call(
    page,
    `/api/admin/applications/${applicationId}/resources/${resourceId}/introspection`,
  );
  assert.equal(metadata.status, 200);
  assert.equal(metadata.body.secret, undefined);
  assert.ok(!JSON.stringify(metadata.body).includes("verifier"));
  const read = randomUUID(),
    write = randomUUID(),
    role = randomUUID();
  // Source-defined authorization fixtures in the disposable browser database only.
  // Validate every interpolated value; no administrator policy-write API exists yet.
  for (const id of [
    applicationId,
    resourceId,
    scopeId,
    principal,
    read,
    write,
    role,
  ])
    assert.match(
      id,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
  await runSql(`BEGIN;
    INSERT INTO capabilities(id,permission_key,meaning) VALUES('${read}','read-${read}','Read records'),('${write}','write-${write}','Write records');
    INSERT INTO capability_applications VALUES('${applicationId}','${read}'),('${applicationId}','${write}');
    INSERT INTO roles(id,name) VALUES('${role}','Writer');
    INSERT INTO role_applications VALUES('${applicationId}','${role}');
    INSERT INTO role_capabilities VALUES('${role}','${read}'),('${role}','${write}');
    INSERT INTO resource_capabilities VALUES('${applicationId}','${resourceId}','${read}'),('${applicationId}','${resourceId}','${write}');
    INSERT INTO scope_capabilities VALUES('${applicationId}','${resourceId}','${scopeId}','${read}'),('${applicationId}','${resourceId}','${scopeId}','${write}');
    INSERT INTO principal_roles VALUES('${principal}','${applicationId}','${role}'); COMMIT;`);
  return {
    name,
    identity,
    resource: resourceId,
    audience: resource.body.record.audience,
    client: client.body.record.id,
    secret: client.body.client_secret,
    credential: credential.body,
    read,
    write,
    role,
  };
}
async function authorize({ page, context, origin, ca, principal, keys }, app) {
  const session = (await context.cookies()).find(
    (c) => c.name === "__Host-darkhorse",
  ).value;
  const verifier = randomBytes(32).toString("base64url");
  const expected = {
    issuer: origin,
    client: app.client,
    subject: principal,
    redirect: `${origin}/callback?resource=${app.resource}`,
    state: randomBytes(32).toString("base64url"),
    nonce: randomBytes(32).toString("base64url"),
  };
  const query = new URLSearchParams({
    client_id: app.client,
    redirect_uri: expected.redirect,
    response_type: "code",
    scope: "openid operate",
    resource: app.audience,
    state: expected.state,
    nonce: expected.nonce,
    code_challenge_method: "S256",
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
  });
  await page.goto(`${origin}/authorize?${query}`);
  await page.getByRole("heading", { name: `Connect ${app.name}?` }).waitFor();
  assert.ok(
    session ===
      (await context.cookies()).find((c) => c.name === "__Host-darkhorse")
        .value,
  );
  await page.getByRole("button", { name: "Allow connection" }).click();
  await page.waitForURL(`${origin}/callback?**`);
  const code = validateCallback(page.url(), expected);
  const token = await exchange(
    origin,
    ca,
    app.client,
    app.secret,
    expected.redirect,
    code,
    verifier,
  );
  assert.equal(token.status, 200);
  validateIdToken(token.body.id_token, keys, {
    ...expected,
    now: Math.floor(Date.now() / 1000),
  });
  assert.equal(
    (await userinfo(origin, ca, token.body.access_token)).status,
    401,
  );
  return { ...token.body, code };
}
function permitted(response, app, capability) {
  return (
    response.status === 200 &&
    response.body.active === true &&
    response.body.aud === app.audience &&
    response.body.capabilities.includes(capability)
  );
}
export async function verifyResourceChecks(options) {
  const { page, origin, ca, call, runSql, principal } = options;
  const first = await register(options, "Inventory");
  const second = await register(options, "Billing");
  const tokens = [
    await authorize(options, first),
    await authorize(options, second),
  ];
  const check = (
    app,
    token,
    endpoint = "introspect",
    secret = app.credential.secret,
    headers = {},
  ) =>
    manage(
      origin,
      ca,
      endpoint,
      app.credential.introspection_client_id,
      secret,
      token,
      "access_token",
      headers,
    );
  for (const [app, own, other] of [
    [first, tokens[0], tokens[1]],
    [second, tokens[1], tokens[0]],
  ]) {
    const active = await check(app, own.access_token);
    assert.equal(active.status, 200);
    assert.equal(active.headers["cache-control"], "no-store");
    assert.deepEqual(active.body, {
      active: true,
      aud: app.audience,
      capabilities: [app.read, app.write].sort(),
      client_id: app.client,
      sub: principal,
      iss: origin,
      scope: "openid operate",
      token_type: "Bearer",
      iat: active.body.iat,
      exp: active.body.exp,
    });
    assert.equal(active.body.exp - active.body.iat, 300);
    assert.equal(permitted(active, app, app.write), true);
    for (const token of [
      other.access_token,
      own.id_token,
      own.code,
      "unknown",
      "eyJ0eXAiOiJsb2dvdXQrand0In0.e30.AA",
    ])
      assert.deepEqual((await check(app, token)).body, { active: false });
    assert.equal(
      (await check(app, "unknown", "introspect", "00".repeat(32))).status,
      401,
    );
    assert.equal((await check(app, own.access_token, "revoke")).status, 401);
    assert.deepEqual(
      (
        await manage(
          origin,
          ca,
          "introspect",
          app.client,
          app.secret,
          own.access_token,
        )
      ).body,
      { active: false },
    );
    for (const headers of [{ origin }, { cookie: "unrelated=1" }])
      assert.equal(
        (
          await check(
            app,
            own.access_token,
            "introspect",
            app.credential.secret,
            headers,
          )
        ).status,
        403,
      );
  }
  const timings = [];
  for (let n = 0; n < 10; n++) {
    const response = await check(first, tokens[0].access_token);
    assert.equal(permitted(response, first, first.write), true);
    timings.push(response.elapsedMs);
  }
  timings.sort((a, b) => a - b);
  console.log(
    `Resource introspection development sample: 10 sequential HTTPS/Basic/primary-state/policy checks, milliseconds p50=${timings[4].toFixed(2)}, p95=${timings[9].toFixed(2)}. Existing login limiter and token HTTP admission remain enabled; no positive cache or distributed introspection limiter. This is not a capacity benchmark.`,
  );
  await runSql(
    `DELETE FROM role_capabilities WHERE role_id='${first.role}' AND capability_id='${first.write}'`,
  );
  const reduced = await check(first, tokens[0].access_token);
  assert.equal(permitted(reduced, first, first.write), false);
  assert.equal(permitted(reduced, first, first.read), true);
  assert.equal(
    permitted(
      await check(second, tokens[1].access_token),
      second,
      second.write,
    ),
    true,
  );
  const rotated = await call(page, "/api/admin/resource-introspection", {
    operation: "rotate",
    ...first.identity,
    revision: 0,
    overlap_seconds: 0,
  });
  assert.equal(rotated.status, 200);
  assert.equal((await check(first, tokens[0].access_token)).status, 401);
  first.credential = rotated.body;
  assert.equal(
    permitted(await check(first, tokens[0].access_token), first, first.read),
    true,
  );
  assert.equal(
    (
      await manage(
        origin,
        ca,
        "revoke",
        first.client,
        first.secret,
        tokens[0].access_token,
      )
    ).status,
    200,
  );
  assert.deepEqual((await check(first, tokens[0].access_token)).body, {
    active: false,
  });
  const disabled = await call(page, "/api/admin/resource-introspection", {
    operation: "set_active",
    ...second.identity,
    revision: 0,
    active: false,
  });
  assert.equal(disabled.status, 200);
  assert.equal(disabled.body.secret, undefined);
  assert.equal((await check(second, tokens[1].access_token)).status, 401);
  console.log(
    "Two-resource HTTPS SSO, dedicated credentials, live capability checks, audience isolation, rotation, disablement and committed revocation passed.",
  );
}
