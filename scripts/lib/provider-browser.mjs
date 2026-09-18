import assert from "node:assert/strict";
import {
  createPublicKey,
  createHash,
  randomBytes,
  generateKeyPairSync,
} from "node:crypto";
import {
  exchange,
  userinfo,
  validateIdToken,
  validateCallback,
} from "./reference-client.mjs";
async function call(page, path, body) {
  return page.evaluate(
    async ({ path, body }) => {
      const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        headers: {
          "content-type": "application/json",
          "x-darkhorse-csrf": "1",
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      return { status: response.status, body: await response.json() };
    },
    { path, body },
  );
}
export async function seedSigning(invoke, docker, db) {
  const staged = JSON.parse((await invoke(["signing-generate", "0"])).stdout);
  assert.match(staged.kid, /^[A-Za-z0-9_-]{43}$/);
  // Advance only this disposable database's publication fixture.
  await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-d",
    "browser_test",
    "-v",
    "ON_ERROR_STOP=1",
    "-c",
    "ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-60001; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;",
  ]);
  const active = JSON.parse(
    (await invoke(["signing-activate", staged.kid, "1"])).stdout,
  );
  assert.equal(active.phase, "active");
  const status = JSON.parse((await invoke(["signing-status"])).stdout);
  assert.equal(status.revision, 2);
  assert.equal(status.keys[0].kid, staged.kid);
  const { privateKey } = generateKeyPairSync("rsa", {
    modulusLength: 3072,
    publicExponent: 65537,
  });
  const der = privateKey.export({ format: "der", type: "pkcs8" });
  try {
    const imported = JSON.parse(
      (await invoke(["signing-import", "--stdin", "2"], der)).stdout,
    );
    assert.equal(imported.phase, "staged");
    const retired = JSON.parse(
      (await invoke(["signing-retire", imported.kid, "3"])).stdout,
    );
    assert.equal(retired.phase, "retired");
    await assert.rejects(
      invoke(["signing-import", "--stdin", "4"], der),
      /conflicts|changed/,
    );
  } finally {
    der.fill(0);
  }
}
export async function verifyProvider(page, context, origin, principal, ca) {
  const metadata = await call(page, "/.well-known/openid-configuration");
  assert.equal(metadata.status, 200);
  assert.equal(metadata.body.issuer, origin);
  assert.equal(metadata.body.token_endpoint, `${origin}/token`);
  assert.equal(metadata.body.userinfo_endpoint, `${origin}/userinfo`);
  assert.deepEqual(metadata.body.token_endpoint_auth_methods_supported, [
    "client_secret_basic",
  ]);
  assert.deepEqual(metadata.body.scopes_supported, ["openid"]);
  const jwks = await call(page, "/jwks");
  assert.equal(jwks.status, 200);
  assert.equal(jwks.body.keys.length, 1);
  const key = jwks.body.keys[0];
  assert.deepEqual(Object.keys(key).sort(), [
    "alg",
    "e",
    "kid",
    "kty",
    "n",
    "use",
  ]);
  assert.equal(
    createPublicKey({ key, format: "jwk" }).asymmetricKeyDetails.modulusLength,
    3072,
  );
  assert.equal(
    key.kid,
    createHash("sha256")
      .update(JSON.stringify({ e: key.e, kty: key.kty, n: key.n }))
      .digest("base64url"),
  );
  const app = await call(page, "/api/admin/registration", {
    operation: "create_application",
    application: { name: "Calendar", owner_id: principal, active: true },
  });
  assert.equal(app.status, 200);
  const registered = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: app.body.record.id,
    client: {
      name: "Calendar",
      active: true,
      redirect_uris: [`${origin}/callback?fixed=1`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(registered.status, 200);
  const verifier = randomBytes(32).toString("base64url");
  const query = new URLSearchParams({
    client_id: registered.body.record.id,
    redirect_uri: `${origin}/callback?fixed=1`,
    response_type: "code",
    scope: "openid",
    state: "state&fixed=x",
    nonce: randomBytes(32).toString("base64url"),
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    code_challenge_method: "S256",
  });
  await page.goto(`${origin}/authorize?${query}`);
  await page.getByRole("heading", { name: "Connect Calendar?" }).waitFor();
  const cookie = (await context.cookies()).find(
    (c) => c.name === "__Host-darkhorse-authorization",
  );
  assert.ok(
    cookie?.secure &&
      cookie.httpOnly &&
      cookie.sameSite === "Lax" &&
      cookie.path === "/",
  );
  assert.ok(
    !(await page.evaluate(() => document.cookie)).includes(
      "__Host-darkhorse-authorization",
    ),
  );
  const first = (await call(page, "/api/authorization")).body;
  await page.screenshot({
    path: ".local/authorization-browser.png",
    fullPage: true,
  });
  await page.goto(`${origin}/authorize?${query}`);
  await page.getByRole("heading", { name: "Connect Calendar?" }).waitFor();
  assert.equal(
    (
      await call(page, "/api/authorization/decision", {
        request_id: first.request_id,
        decision: "approve",
      })
    ).status,
    400,
  );
  assert.equal(
    await page.evaluate(
      async () =>
        (
          await fetch("/api/authorization/decision", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: "{}",
          })
        ).status,
    ),
    403,
  );
  await page.route(`${origin}/callback?**`, (route) =>
    route.fulfill({
      status: 200,
      body: "Returned to application",
      contentType: "text/plain",
    }),
  );
  await page.getByRole("button", { name: "Allow connection" }).click();
  await page.waitForURL(`${origin}/callback?**`);
  const expected = {
    issuer: origin,
    client: registered.body.record.id,
    subject: principal,
    nonce: query.get("nonce"),
    state: query.get("state"),
    redirect: query.get("redirect_uri"),
  };
  const code = validateCallback(page.url(), expected);
  const redeem = (
    value = code,
    secret = registered.body.client_secret,
    proof = verifier,
    headers = {},
  ) =>
    exchange(
      origin,
      ca,
      expected.client,
      secret,
      expected.redirect,
      value,
      proof,
      headers,
    );
  assert.equal((await redeem(code, "00".repeat(32))).status, 401);
  assert.equal(
    (await redeem(code, registered.body.client_secret, "z".repeat(43))).status,
    400,
  );
  for (const headers of [{ origin }, { cookie: "unrelated=1" }])
    assert.equal(
      (await redeem(code, registered.body.client_secret, verifier, headers))
        .status,
      403,
    );
  const issued = await redeem();
  assert.equal(issued.status, 200);
  assert.equal(issued.headers["cache-control"], "no-store");
  assert.equal(issued.headers.pragma, "no-cache");
  assert.match(issued.body.access_token, /^da_[a-f0-9]{64}$/);
  assert.equal(issued.body.refresh_token, undefined);
  assert.equal(issued.body.token_type, "Bearer");
  assert.equal(issued.body.expires_in, 300);
  const validation = { ...expected, now: Math.floor(Date.now() / 1000) };
  validateIdToken(issued.body.id_token, jwks.body.keys, validation);
  for (const changed of [
    { issuer: "https://other.example" },
    { client: principal },
    { nonce: "substitution" },
    { now: validation.now + 301 },
  ]) {
    assert.throws(() =>
      validateIdToken(issued.body.id_token, jwks.body.keys, {
        ...validation,
        ...changed,
      }),
    );
  }
  const info = await userinfo(origin, ca, issued.body.access_token);
  assert.equal(info.status, 200);
  assert.deepEqual(info.body, { sub: principal });
  for (const credential of [
    issued.body.id_token,
    code,
    "eyJ0eXAiOiJsb2dvdXQrand0In0.eyJldmVudHMiOnt9fQ.signature",
  ]) {
    assert.equal((await userinfo(origin, ca, credential)).status, 401);
  }
  // An authenticated replay rejects and revokes the issued access credential.
  assert.equal((await redeem()).status, 400);
  assert.equal(
    (await userinfo(origin, ca, issued.body.access_token)).status,
    401,
  );
  query.set("prompt", "none");
  await page.goto(`${origin}/authorize?${query}`);
  const silentCode = validateCallback(page.url(), expected);
  assert.ok(silentCode !== code);
  assert.equal((await redeem(silentCode)).status, 200);
  await page.goto(`${origin}/authorize?${query}`);
  const lostCode = validateCallback(page.url(), expected);
  const discarded = await exchange(
    origin,
    ca,
    expected.client,
    registered.body.client_secret,
    expected.redirect,
    lostCode,
    verifier,
    {},
    true,
  );
  assert.equal(discarded.status, 200);
  assert.equal((await redeem(lostCode)).status, 400);
  let returned;
  query.set("prompt", "consent");
  await page.goto(`${origin}/authorize?${query}`);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.waitForURL(`${origin}/callback?**`);
  returned = new URL(page.url());
  assert.equal(returned.searchParams.get("error"), "access_denied");
  query.set("redirect_uri", "https://unregistered.example/callback");
  const denied = await page.goto(`${origin}/authorize?${query}`);
  assert.equal(denied.status(), 400);
  assert.equal(new URL(page.url()).origin, origin);
  await page.goto(origin);
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  console.log(
    "Provider HTTPS discovery, PKCE code exchange, independent RS256 validation, UserInfo, replay revocation, consent and substitution checks passed.",
  );
}
