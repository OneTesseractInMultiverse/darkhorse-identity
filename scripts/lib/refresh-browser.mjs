import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import {
  exchange,
  refresh,
  manage,
  userinfo,
  validateCallback,
  validateIdToken,
} from "./reference-client.mjs";

export async function verifyRefresh({
  page,
  origin,
  ca,
  principal,
  call,
  keys,
}) {
  const application = await call(page, "/api/admin/registration", {
    operation: "create_application",
    application: { name: "Refresh", owner_id: principal, active: true },
  });
  assert.equal(application.status, 200);
  const registered = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: application.body.record.id,
    client: {
      name: "Refresh",
      active: true,
      refresh_tokens: true,
      redirect_uris: [`${origin}/callback?fixed=refresh`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(registered.status, 200);
  assert.equal(registered.body.record.refresh_tokens, true);
  const client = registered.body.record.id;
  const secret = registered.body.client_secret;
  const verifier = randomBytes(32).toString("base64url");
  const expected = {
    issuer: origin,
    client,
    subject: principal,
    redirect: `${origin}/callback?fixed=refresh`,
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
    prompt: "consent",
  });
  const authorize = async () => {
    await page.goto(`${origin}/authorize?${query}`);
    await page.getByRole("heading", { name: "Connect Refresh?" }).waitFor();
    await page.getByRole("button", { name: "Allow connection" }).click();
    await page.waitForURL(`${origin}/callback?**`);
    const code = validateCallback(page.url(), expected);
    const result = await exchange(
      origin,
      ca,
      client,
      secret,
      expected.redirect,
      code,
      verifier,
    );
    assert.equal(result.status, 200);
    assert.match(result.body.refresh_token, /^dr_[a-f0-9]{64}$/);
    validateIdToken(result.body.id_token, keys, {
      ...expected,
      now: Math.floor(Date.now() / 1000),
    });
    return result.body;
  };
  const rotate = (token, scope, headers = {}) =>
    refresh(origin, ca, client, secret, token, scope, headers);
  const initial = await authorize();
  const inactive = await manage(
    origin,
    ca,
    "introspect",
    client,
    secret,
    initial.refresh_token,
    "access_token",
  );
  assert.deepEqual(inactive.body, { active: false });
  assert.equal((await userinfo(origin, ca, initial.refresh_token)).status, 401);
  assert.equal((await rotate(initial.access_token)).status, 400);
  assert.equal(
    (await refresh(origin, ca, client, "00".repeat(32), initial.refresh_token))
      .status,
    401,
  );
  for (const headers of [{ origin }, { cookie: "unrelated=1" }])
    assert.equal(
      (await rotate(initial.refresh_token, undefined, headers)).status,
      403,
    );
  const narrowed = await rotate(initial.refresh_token, "openid email");
  assert.equal(narrowed.status, 200);
  assert.equal(narrowed.headers["cache-control"], "no-store");
  assert.equal(narrowed.headers.pragma, "no-cache");
  assert.deepEqual(Object.keys(narrowed.body).sort(), [
    "access_token",
    "expires_in",
    "refresh_token",
    "scope",
    "token_type",
  ]);
  assert.equal(narrowed.body.scope, "openid email");
  assert.equal(narrowed.body.expires_in, 300);
  assert.notEqual(narrowed.body.refresh_token, initial.refresh_token);
  const expanded = await rotate(
    narrowed.body.refresh_token,
    "openid profile email",
  );
  assert.equal(expanded.status, 400);
  assert.deepEqual(expanded.body, { error: "invalid_scope" });
  const next = await rotate(narrowed.body.refresh_token);
  assert.equal(next.status, 200);
  const profile = await userinfo(origin, ca, next.body.access_token);
  assert.deepEqual(profile.body, {
    sub: principal,
    email: "browser@example.com",
    email_verified: false,
  });
  const replay = await rotate(initial.refresh_token);
  assert.equal(replay.status, 400);
  assert.deepEqual(replay.body, { error: "invalid_grant" });
  assert.equal(
    (await userinfo(origin, ca, next.body.access_token)).status,
    401,
  );
  const lost = await authorize();
  assert.equal(
    (
      await refresh(
        origin,
        ca,
        client,
        secret,
        lost.refresh_token,
        undefined,
        {},
        true,
      )
    ).status,
    200,
  );
  assert.equal((await rotate(lost.refresh_token)).status, 400);
  assert.equal((await userinfo(origin, ca, lost.access_token)).status, 401);
  const revoked = await authorize();
  assert.equal(
    (
      await manage(
        origin,
        ca,
        "revoke",
        client,
        secret,
        revoked.refresh_token,
        "access_token",
      )
    ).status,
    200,
  );
  assert.equal((await rotate(revoked.refresh_token)).status, 400);
  assert.equal((await userinfo(origin, ca, revoked.access_token)).status, 401);
  console.log(
    "HTTPS refresh opt-in, rotation, purpose isolation, narrowed disclosure, replay, lost response and family revocation checks passed.",
  );
}
