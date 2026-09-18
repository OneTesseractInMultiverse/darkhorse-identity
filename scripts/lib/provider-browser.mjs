import assert from "node:assert/strict";
import {
  createPublicKey,
  createHash,
  randomBytes,
  generateKeyPairSync,
} from "node:crypto";
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
export async function verifyProvider(page, context, origin, principal) {
  const metadata = await call(page, "/.well-known/openid-configuration");
  assert.equal(metadata.status, 503);
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
  const query = new URLSearchParams({
    client_id: registered.body.record.id,
    redirect_uri: `${origin}/callback?fixed=1`,
    response_type: "code",
    scope: "openid",
    state: "state&fixed=x",
    nonce: randomBytes(32).toString("base64url"),
    code_challenge: createHash("sha256")
      .update(randomBytes(32))
      .digest("base64url"),
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
  await page.getByRole("button", { name: "Allow connection" }).click();
  await page.getByRole("heading", { name: "Unable to connect" }).waitFor();
  assert.equal((await call(page, "/api/authorization")).body.status, "ready");
  await page.route(`${origin}/callback?**`, (route) =>
    route.fulfill({
      status: 200,
      body: "Returned to application",
      contentType: "text/plain",
    }),
  );
  query.set("prompt", "none");
  await page.goto(`${origin}/authorize?${query}`);
  let returned = new URL(page.url());
  assert.equal(returned.searchParams.get("error"), "temporarily_unavailable");
  assert.equal(returned.searchParams.get("state"), "state&fixed=x");
  assert.equal(returned.searchParams.get("fixed"), "1");
  assert.equal(returned.searchParams.has("code"), false);
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
    "Provider HTTPS, independent JWK import/thumbprint, consent, session cookie, substitution, silent response and redirect checks passed.",
  );
}
