import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import {
  exchange,
  userinfo,
  manage,
  validateCallback,
  validateIdToken,
} from "./reference-client.mjs";

export async function verifyIdentityChecks({
  page,
  context,
  origin,
  ca,
  principal,
  call,
  first,
  firstToken,
  keys,
}) {
  const session = (await context.cookies()).find(
    (c) => c.name === "__Host-darkhorse",
  ).value;
  const application = await call(page, "/api/admin/registration", {
    operation: "create_application",
    application: { name: "Reports", owner_id: principal, active: true },
  });
  assert.equal(application.status, 200);
  const registered = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: application.body.record.id,
    client: {
      name: "Reports",
      active: true,
      redirect_uris: [`${origin}/callback?fixed=2`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(registered.status, 200);
  const client = registered.body.record.id;
  const secret = registered.body.client_secret;
  const verifier = randomBytes(32).toString("base64url");
  const expected = {
    issuer: origin,
    client,
    subject: principal,
    redirect: `${origin}/callback?fixed=2`,
    state: randomBytes(32).toString("base64url"),
    nonce: randomBytes(32).toString("base64url"),
  };
  const query = new URLSearchParams({
    client_id: client,
    redirect_uri: expected.redirect,
    response_type: "code",
    scope: "openid profile email",
    state: expected.state,
    nonce: expected.nonce,
    code_challenge_method: "S256",
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
  });
  await page.goto(`${origin}/authorize?${query}`);
  // The second application requires consent, but shares the existing login.
  await page.getByRole("heading", { name: "Connect Reports?" }).waitFor();
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
    client,
    secret,
    expected.redirect,
    code,
    verifier,
  );
  assert.equal(token.status, 200);
  assert.equal(token.body.scope, "openid profile email");
  validateIdToken(token.body.id_token, keys, {
    ...expected,
    now: Math.floor(Date.now() / 1000),
  });
  assert.deepEqual((await userinfo(origin, ca, firstToken)).body, {
    sub: principal,
  });
  assert.deepEqual((await userinfo(origin, ca, token.body.access_token)).body, {
    sub: principal,
    name: "Browser Test",
    given_name: "Browser",
    family_name: "Test",
    email: "browser@example.com",
    email_verified: false,
  });
  const check = (
    access,
    endpoint = "introspect",
    password = secret,
    headers = {},
  ) =>
    manage(
      origin,
      ca,
      endpoint,
      client,
      password,
      access,
      "refresh_token",
      headers,
    );
  const active = await check(token.body.access_token);
  assert.equal(active.status, 200);
  assert.equal(active.headers["cache-control"], "no-store");
  assert.equal(active.headers.pragma, "no-cache");
  assert.deepEqual(active.body, {
    active: true,
    aud: `${origin}/userinfo`,
    client_id: client,
    sub: principal,
    iss: origin,
    scope: "openid profile email",
    token_type: "Bearer",
    iat: active.body.iat,
    exp: active.body.exp,
  });
  assert.equal(active.body.exp - active.body.iat, 300);
  assert.deepEqual((await check(firstToken)).body, { active: false });
  assert.deepEqual(
    (
      await manage(
        origin,
        ca,
        "introspect",
        first.record.id,
        first.client_secret,
        token.body.access_token,
      )
    ).body,
    { active: false },
  );
  assert.equal((await check(firstToken, "revoke")).status, 200);
  assert.equal((await userinfo(origin, ca, firstToken)).status, 200);
  for (const input of [
    token.body.id_token,
    code,
    "unknown",
    "eyJ0eXAiOiJsb2dvdXQrand0In0.e30.AA",
  ]) {
    assert.deepEqual((await check(input)).body, { active: false });
    const revoked = await check(input, "revoke");
    assert.equal(revoked.status, 200);
    assert.equal(revoked.body, null);
    const denied = await check(input, "introspect", "00".repeat(32));
    assert.equal(denied.status, 401);
    assert.deepEqual(denied.body, { error: "invalid_client" });
  }
  for (const endpoint of ["introspect", "revoke"]) {
    for (const headers of [{ origin }, { cookie: "unrelated=1" }])
      assert.equal(
        (await check(token.body.access_token, endpoint, secret, headers))
          .status,
        403,
      );
  }
  const timings = [];
  for (let n = 0; n < 10; n++) {
    const measured = await check(token.body.access_token);
    assert.equal(measured.body.active, true);
    timings.push(measured.elapsedMs);
  }
  timings.sort((a, b) => a - b);
  console.log(
    `Introspection development sample: 10 sequential HTTPS/Basic/primary-state checks, milliseconds p50=${timings[4].toFixed(2)}, p95=${timings[9].toFixed(2)}; limiter-enabled login established the session. Introspection uses shared deployment/caller budgets and HTTP admission bounds. This is not a capacity benchmark.`,
  );
  for (let n = 0; n < 2; n++) {
    const revoked = await check(token.body.access_token, "revoke");
    assert.equal(revoked.status, 200);
    assert.equal(revoked.body, null);
  }
  assert.deepEqual((await check(token.body.access_token)).body, {
    active: false,
  });
  assert.equal(
    (await userinfo(origin, ca, token.body.access_token)).status,
    401,
  );
  assert.equal((await userinfo(origin, ca, firstToken)).status, 200);
  console.log(
    "Two-application SSO, scoped UserInfo, authenticated introspection, client isolation, purpose rejection and committed revocation checks passed.",
  );
}
