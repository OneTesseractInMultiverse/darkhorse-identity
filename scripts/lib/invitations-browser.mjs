import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { randomBytes } from "node:crypto";
import { resolve } from "node:path";
import { call } from "./provider-browser.mjs";
export async function verifyInvitations(
  browser,
  administrator,
  origin,
  mailbox,
) {
  const start = mailbox.messages.length;
  const created = await call(administrator, "/api/admin/invitations", {
    email: "invited@example.com",
    locale: "es",
  });
  assert.equal(created.status, 201);
  assert.equal(typeof created.body.id, "string");
  assert.deepEqual(Object.keys(created.body), ["id"]);
  for (
    let attempt = 0;
    attempt < 100 && mailbox.messages.length === start;
    attempt++
  )
    await delay(100);
  assert.equal(mailbox.messages.length, start + 1);
  const text = Buffer.from(
    mailbox.messages[start]
      .replace(/=\r\n/g, "")
      .replace(/=([0-9A-F]{2})/g, (_, hex) =>
        String.fromCharCode(parseInt(hex, 16)),
      ),
    "binary",
  ).toString("utf8");
  assert.ok(text.includes("24 horas"));
  assert.ok(text.includes("contraseña"));
  const match = text.match(
    /https:\/\/localhost:[0-9]+\/invitation#token=(iv1_[a-f0-9]{64})&lang=es/,
  );
  assert.ok(
    match,
    "SMTP must deliver an invitation with its own proof purpose",
  );
  const token = match[1];
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const urls = [];
    const errors = [];
    page.on("request", (request) => urls.push(request.url()));
    page.on("pageerror", () => errors.push("page error"));
    await page.goto(match[0]);
    await page.getByRole("form", { name: "Aceptar invitación" }).waitFor();
    assert.equal(
      await page.evaluate(() => document.documentElement.lang),
      "es",
    );
    assert.equal(
      await page.evaluate(() => localStorage.getItem("darkhorse.locale.v1")),
      null,
    );
    await page.getByRole("combobox").selectOption("en");
    assert.equal(page.url(), `${origin}/invitation`);
    assert.equal((await call(page, "/api/admin/invitations")).status, 401);
    const password = randomBytes(24).toString("base64url");
    for (const [label, value] of [
      ["Email address", "invited@example.com"],
      ["First name", "Invited"],
      ["Last name", "Person"],
      ["Password", password],
      ["Confirm password", password],
    ])
      await page.getByLabel(label, { exact: true }).fill(value);
    await page.getByRole("combobox").selectOption("es");
    assert.equal(
      await page.getByLabel("Contraseña", { exact: true }).inputValue(),
      password,
    );
    await page.screenshot({
      path: resolve(".local/invitation-desktop.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      true,
    );
    await page.screenshot({
      path: resolve(".local/invitation-mobile.png"),
      fullPage: true,
    });
    await page
      .getByRole("button", { name: "Crear cuenta", exact: true })
      .click();
    await page.getByText(/Tu cuenta está lista/).waitFor();
    assert.equal(
      (await call(page, "/api/auth/session")).status,
      401,
      "acceptance must not establish a login session",
    );
    assert.equal(
      (
        await call(page, "/api/invitations/accept", {
          token,
          email: "invited@example.com",
          first_name: "Invited",
          last_name: "Person",
          password,
        })
      ).status,
      400,
    );
    await page.getByRole("link", { name: "Ir al inicio de sesión" }).click();
    await page.getByLabel("Correo electrónico").fill("invited@example.com");
    await page.getByLabel("Contraseña", { exact: true }).fill(password);
    await page
      .getByRole("button", { name: "Iniciar sesión", exact: true })
      .click();
    await page.getByRole("heading", { name: /bienvenida, Invited/ }).waitFor();
    assert.equal((await call(page, "/api/security/email")).body.verified, true);
    assert.equal(
      (await call(page, "/api/admin/invitations")).status,
      403,
      "new accounts must not inherit administrator membership",
    );
    assert.equal(
      (
        await call(page, "/api/admin/invitations", {
          email: "escalation@example.com",
        })
      ).status,
      403,
    );
    assert.ok(
      urls.every((url) => !url.includes(token)),
      "proof must not enter request URLs",
    );
    assert.equal(
      await page.evaluate(
        (proof) =>
          [
            ...Object.values(localStorage),
            ...Object.values(sessionStorage),
          ].some((value) => value.includes(proof)),
        token,
      ),
      false,
    );
    const rows = (await call(administrator, "/api/admin/invitations")).body
      .invitations;
    assert.equal(rows.find((row) => row.id === created.body.id).closed, true);
    const revoke = await call(administrator, "/api/admin/invitations", {
      email: "cancelled@example.com",
    });
    assert.equal(revoke.status, 201);
    assert.equal(
      (
        await call(administrator, "/api/admin/invitations/revoke", {
          id: revoke.body.id,
        })
      ).status,
      200,
    );
    assert.equal(
      (
        await call(administrator, "/api/admin/invitations", {
          email: "INVITED@example.com",
        })
      ).status,
      409,
    );
    assert.deepEqual(errors, []);
    console.log(
      "HTTPS invitation issuance, TLS delivery, fragment removal, single-use enrollment, normal sign-in and administrator isolation passed.",
    );
  } finally {
    await context.close();
  }
}
