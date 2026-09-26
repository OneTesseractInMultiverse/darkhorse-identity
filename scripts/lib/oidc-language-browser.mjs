import assert from "node:assert/strict";
import { expect } from "@playwright/test";
import { createHash, randomBytes } from "node:crypto";

export async function verifyOidcLanguages({
  page,
  context,
  origin,
  application,
  client,
  call,
}) {
  const created = await call(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: application,
    client: {
      name: "English Workspace",
      active: true,
      redirect_uris: [`${origin}/callback?fixed=1`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(created.status, 200);
  const isolated = await context.browser().newContext({ locale: "en-US" });
  try {
    // Copy only the current disposable authentication cookie, never another flow's proof or browser preference.
    await isolated.addCookies(
      (await context.cookies()).filter(
        (cookie) => cookie.name === "__Host-darkhorse",
      ),
    );
    await isolated.route(`${origin}/callback?**`, (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/plain",
        body: "Returned to test application",
      }),
    );
    const spanish = await isolated.newPage(),
      english = await isolated.newPage();
    const begin = (id, locales, state) =>
      `${origin}/authorize?${new URLSearchParams({
        client_id: id,
        redirect_uri: `${origin}/callback?fixed=1`,
        response_type: "code",
        scope: "openid",
        prompt: "consent",
        state,
        nonce: randomBytes(16).toString("hex"),
        code_challenge_method: "S256",
        code_challenge: createHash("sha256")
          .update(randomBytes(32))
          .digest("base64url"),
        ui_locales: locales,
      })}`;
    await spanish.goto(begin(client, "es-CR es en", "spanish-tab"));
    await expect(
      spanish.getByRole("heading", {
        name: "¿Conectar Calendar?",
        exact: true,
      }),
    ).toBeVisible();
    const es = new URL(spanish.url()).searchParams.get("request");
    assert.match(es, /^[a-f0-9]{64}$/);
    await english.goto(
      begin(created.body.record.id, "en-GB es", "english-tab"),
    );
    await expect(
      english.getByRole("heading", {
        name: "Connect English Workspace?",
        exact: true,
      }),
    ).toBeVisible();
    const en = new URL(english.url()).searchParams.get("request");
    assert.match(en, /^[a-f0-9]{64}$/);
    assert.notEqual(es, en);
    await spanish.reload();
    await expect(
      spanish.getByRole("heading", {
        name: "¿Conectar Calendar?",
        exact: true,
      }),
    ).toBeVisible();
    assert.equal(
      (await call(spanish, `/api/authorization?request=${es}`)).body.ui_locale,
      "es",
    );
    assert.equal(
      (await call(english, `/api/authorization?request=${en}`)).body.ui_locale,
      "en",
    );
    assert.equal(
      await spanish.evaluate(() => localStorage.length),
      0,
      "transaction hints must not persist an anonymous preference",
    );
    const before = (await call(spanish, "/api/profiles/me")).body
      .preferred_locale;
    assert.equal(
      (
        await call(english, `/api/authorization/decision?request=${en}`, {
          request_id: es,
          decision: "approve",
        })
      ).status,
      400,
    );
    await spanish.getByRole("combobox").selectOption("en");
    await expect(
      spanish.getByRole("heading", { name: "Connect Calendar?", exact: true }),
    ).toBeVisible();
    assert.equal(
      (await call(spanish, "/api/profiles/me")).body.preferred_locale,
      before,
    );
    assert.equal(
      (await call(spanish, `/api/authorization?request=${es}`)).body.ui_locale,
      "es",
    );
    await english.getByRole("button", { name: "Cancel", exact: true }).click();
    await english.waitForURL(`${origin}/callback?**`);
    assert.equal(
      new URL(english.url()).searchParams.get("state"),
      "english-tab",
    );
    assert.equal(
      new URL(english.url()).searchParams.get("error"),
      "access_denied",
    );
    const cookies = await isolated.cookies();
    assert.ok(
      cookies.some(
        (cookie) => cookie.name === `__Host-darkhorse-authorization-${es}`,
      ),
    );
    assert.ok(
      !cookies.some(
        (cookie) => cookie.name === `__Host-darkhorse-authorization-${en}`,
      ),
    );
    await spanish.reload();
    await expect(
      spanish.getByRole("heading", { name: "Connect Calendar?", exact: true }),
    ).toBeVisible();
    await spanish.getByRole("button", { name: "Cancel", exact: true }).click();
    await spanish.waitForURL(`${origin}/callback?**`);
    assert.equal(
      new URL(spanish.url()).searchParams.get("state"),
      "spanish-tab",
    );
    await english.goto(
      begin(created.body.record.id, "de-DE fr-CA", "unsupported"),
    );
    await expect(
      english.getByRole("heading", {
        name: "Connect English Workspace?",
        exact: true,
      }),
    ).toBeVisible();
    const fallback = new URL(english.url()).searchParams.get("request");
    assert.equal(
      (await call(english, `/api/authorization?request=${fallback}`)).body
        .ui_locale,
      null,
    );
    await english.getByRole("button", { name: "Cancel", exact: true }).click();
    await english.waitForURL(`${origin}/callback?**`);
  } finally {
    await isolated.close();
  }
}
