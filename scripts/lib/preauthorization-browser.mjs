import assert from "node:assert/strict";
import { expect } from "@playwright/test";

export async function verifyPreauthorization({
  page,
  context,
  origin,
  application,
  password,
  call,
  source,
  runSql,
}) {
  // Isolate login budgets and session counts from the administrator scenarios.
  const principal = "00000000-0000-0000-0000-00000000e071";
  const credential = "00000000-0000-0000-0000-00000000e072";
  await runSql(`INSERT INTO principals(id,email,first_name,last_name) VALUES('${principal}','continuation@example.com','Continuation','Test');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${principal}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;`);
  const callback = `${origin}/continuation-callback?padding=${"x".repeat(1900 - origin.length)}`;
  const registered = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: application,
    client: {
      name: "Continuation fixture",
      active: true,
      redirect_uris: [callback],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(registered.status, 200);
  const isolated = await context.browser().newContext({ locale: "en-US" });
  try {
    const tab = await isolated.newPage();
    await tab.route(`${origin}/continuation-callback?**`, (route) =>
      route.fulfill({ status: 200, body: "Returned to application" }),
    );
    const query = new URLSearchParams({
      client_id: registered.body.record.id,
      redirect_uri: callback,
      response_type: "code",
      scope: "openid",
      prompt: "login consent",
      max_age: "0",
      state: "&".repeat(512),
      nonce: "?".repeat(256),
      code_challenge_method: "S256",
      code_challenge: "A".repeat(43),
      ui_locales: "en",
    });
    const original = query.toString();
    const bounded = `${original}&unused=${"x".repeat(8192 - original.length - 8)}`;
    assert.equal(bounded.length, 8192);
    for (const existingSession of [false, true]) {
      await tab.goto(`${origin}/authorize?${bounded}`);
      await expect(
        tab.getByLabel("Email address", { exact: true }),
      ).toBeVisible();
      const reference = new URL(tab.url()).searchParams.get("request");
      assert.match(reference, /^[a-f0-9]{64}\.[A-Za-z0-9_-]+$/);
      assert.ok(reference.length > 4096 && reference.length <= 24_576);
      const proof = (await isolated.cookies()).find(
        (cookie) =>
          cookie.name ===
          `__Host-darkhorse-authorization-${reference.slice(0, 64)}`,
      );
      assert.ok(proof?.secure && proof.httpOnly && proof.sameSite === "Lax");
      assert.equal(
        (await call(tab, `/api/authorization?request=${reference}`)).body
          .status,
        "login",
      );
      await tab.reload();
      await tab
        .getByLabel("Email address", { exact: true })
        .fill("continuation@example.com");
      await tab.getByLabel("Password", { exact: true }).fill(password);
      const authentication = tab.waitForResponse(
        (response) =>
          response.url() === `${origin}/api/auth/login` &&
          response.request().method() === "POST",
      );
      await tab.getByRole("button", { name: "Sign in", exact: true }).click();
      assert.equal(
        (await authentication).status(),
        200,
        "the fixture sign-in must succeed before checking continuation",
      );
      const resumed = await call(
        tab,
        `/api/authorization?request=${reference}`,
      );
      assert.equal(
        resumed.status,
        200,
        "authenticated continuation must remain valid",
      );
      assert.equal(
        resumed.body.status,
        "consent",
        "fresh authentication must satisfy forced login",
      );
      // An account's preferred language may replace the request hint after sign-in.
      await expect(
        tab.getByRole("button", {
          name: /^(Allow connection|Permitir conexión)$/,
        }),
      ).toBeVisible();
      await tab
        .getByRole("button", { name: /^(Allow connection|Permitir conexión)$/ })
        .click();
      await tab.waitForURL(`${origin}/continuation-callback?**`);
      const returned = new URL(tab.url());
      assert.equal(returned.searchParams.get("state"), query.get("state"));
      assert.equal(returned.searchParams.get("iss"), origin);
      assert.ok(returned.searchParams.get("code"));
      assert.ok(
        !(await isolated.cookies()).some(
          (cookie) => cookie.name === proof.name,
        ),
      );
      assert.equal(
        (await call(tab, `/api/authorization?request=${reference}`)).status,
        400,
      );
      assert.equal(await tab.evaluate(() => localStorage.length), 0);
      console.log(
        `HTTPS large continuation completed with ${existingSession ? "forced reauthentication" : "fresh sign-in"}.`,
      );
    }
  } finally {
    await isolated.close();
  }
}
