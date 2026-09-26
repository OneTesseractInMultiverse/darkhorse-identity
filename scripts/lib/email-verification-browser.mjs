import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { resolve } from "node:path";
import { call } from "./provider-browser.mjs";
export async function verifyEmail(page, origin, mailbox) {
  await page.goto(`${origin}/security/email`);
  await page.getByText("Email not verified", { exact: true }).waitFor();
  assert.equal(mailbox.messages.length, 0);
  const profile = await call(page, "/api/profiles/me");
  assert.equal(
    (
      await call(page, "/api/profiles/me/language", {
        revision: profile.body.revision,
        locale: "es",
      })
    ).status,
    200,
  );
  await page.getByRole("button", { name: "Send verification email" }).click();
  await page.getByText(/A verification message has been queued/).waitFor();
  for (
    let attempt = 0;
    attempt < 100 && mailbox.messages.length === 0;
    attempt++
  )
    await delay(100);
  assert.equal(mailbox.messages.length, 1, "TLS SMTP delivery must complete");
  // Lettre emits quoted-printable plain text; undo wrapping and byte escapes.
  const text = Buffer.from(
    mailbox.messages[0]
      .replace(/=\r\n/g, "")
      .replace(/=([0-9A-F]{2})/g, (_, hex) =>
        String.fromCharCode(parseInt(hex, 16)),
      ),
    "binary",
  ).toString("utf8");
  assert.ok(text.includes("15 minutos"));
  assert.ok(text.includes("contraseña"));
  const match = text.match(
    /https:\/\/localhost:[0-9]+\/security\/email#token=(ev1_[a-f0-9]{64})&lang=es/,
  );
  assert.ok(match, "message must contain the canonical verification link");
  const token = match[1];
  // A first document load also exercises hydration before router history replacement.
  const fresh = await page.context().newPage();
  const freshErrors = [];
  fresh.on("pageerror", () => freshErrors.push("page error"));
  try {
    await fresh.goto(match[0]);
    await fresh.getByRole("button", { name: "Confirmar correo" }).waitFor();
    await fresh.getByRole("combobox").selectOption("es");
    await fresh
      .getByRole("button", { name: "Confirmar correo", exact: true })
      .waitFor();
    assert.equal(fresh.url(), `${origin}/security/email`);
    assert.deepEqual(freshErrors, []);
  } finally {
    await fresh.close();
  }

  await page.getByRole("combobox").selectOption("en");
  assert.equal((await call(page, "/api/security/email")).body.verified, false);
  const urls = [];
  const listener = (request) => urls.push(request.url());
  page.on("request", listener);
  await page.goto(match[0]);
  await page.getByRole("button", { name: "Confirm email" }).waitFor();
  await page.getByRole("combobox").selectOption("es");
  await page
    .getByRole("button", { name: "Confirmar correo", exact: true })
    .waitFor();
  assert.equal(page.url(), `${origin}/security/email`);
  assert.equal((await call(page, "/api/security/email")).body.verified, false);
  await page.screenshot({
    path: resolve(".local/email-verification-desktop.png"),
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
    path: resolve(".local/email-verification-mobile.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Confirmar correo" }).click();
  await page.getByText("Correo verificado", { exact: true }).waitFor();
  assert.equal((await call(page, "/api/security/email")).body.verified, true);
  assert.equal(
    (await call(page, "/api/security/email/confirm", { token })).status,
    400,
  );
  assert.ok(
    urls.every((url) => !url.includes(token)),
    "proof must remain absent from request URLs",
  );
  assert.equal(
    await page.evaluate(
      (proof) =>
        [...Object.values(localStorage), ...Object.values(sessionStorage)].some(
          (value) => value.includes(proof),
        ),
      token,
    ),
    false,
  );
  page.off("request", listener);
  await page.setViewportSize({ width: 1280, height: 900 });
  const updated = await call(page, "/api/profiles/me");
  assert.equal(
    (
      await call(page, "/api/profiles/me/language", {
        revision: updated.body.revision,
        locale: null,
      })
    ).status,
    200,
  );
  await page.getByRole("combobox").selectOption("en");
  await page.goto(origin);
  console.log(
    "HTTPS email request, authenticated TLS SMTP, fragment removal, explicit single-use confirmation and mobile bounds passed.",
  );
}
