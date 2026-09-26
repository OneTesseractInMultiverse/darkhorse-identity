import assert from "node:assert/strict";
import { resolve } from "node:path";
import { expect } from "@playwright/test";

export async function verifyLocalization(browser, origin) {
  const spanish = await browser.newContext({
    locale: "es-CR",
    viewport: { width: 390, height: 844 },
  });
  const english = await browser.newContext({ locale: "en-US" });
  const blocked = await browser.newContext({ locale: "en-US" });
  try {
    const page = await spanish.newPage();
    const errors = [];
    page.on("pageerror", () => errors.push("page error"));
    const url = `${origin}/?presentation=test#unchanged-fragment`;
    await page.goto(url);
    await expect(
      page.getByRole("combobox", { name: "Idioma", exact: true }),
    ).toHaveValue("es");
    await expect(page.locator("html")).toHaveAttribute("lang", "es");
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
    assert.equal(page.url(), url);
    await page
      .getByLabel("Correo electrónico", { exact: true })
      .fill("browser@example.com");
    await page
      .getByLabel("Contraseña", { exact: true })
      .fill("intentionally-incorrect");
    const selector = page.getByRole("combobox");
    await selector.focus();
    await page.keyboard.press("e");
    await page.keyboard.press("Enter");
    await expect(selector).toHaveValue("en");
    await expect(page.getByLabel("Password", { exact: true })).toHaveValue(
      "intentionally-incorrect",
    );
    await selector.selectOption("es");
    await expect(selector).toHaveValue("es");
    await expect(selector).toBeFocused();
    await page
      .getByRole("button", { name: "Iniciar sesión", exact: true })
      .click();
    const alert = page.getByRole("alert");
    await expect(alert).toHaveText(
      "No se pudo iniciar sesión con esas credenciales.",
    );
    await expect(alert).toBeFocused();
    await expect(page.getByLabel("Contraseña", { exact: true })).toHaveValue(
      "",
    );
    await selector.selectOption("en");
    await expect(alert).toHaveText("Unable to sign in with those credentials.");
    await page.reload();
    await expect(
      page.getByRole("combobox", { name: "Language", exact: true }),
    ).toHaveValue("en");
    assert.equal(page.url(), url);
    await selector.selectOption("es");
    await page.reload();
    await expect(selector).toHaveValue("es");
    assert.equal(
      await page.evaluate(() => localStorage.getItem("darkhorse.locale.v1")),
      "es",
    );
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    );
    assert.equal(overflow, false, "Spanish sign-in must fit a narrow viewport");
    await page.screenshot({
      path: resolve(".local/login-es-mobile.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.screenshot({
      path: resolve(".local/login-es-desktop.png"),
      fullPage: true,
    });
    await page.goto(`${origin}/account/profile`);
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    const other = await english.newPage();
    await other.goto(origin);
    await expect(other.getByRole("combobox")).toHaveValue("en");
    await expect(other.locator("html")).toHaveAttribute("lang", "en");
    await blocked.addInitScript(() => {
      Object.defineProperty(window, "localStorage", {
        get() {
          throw new DOMException("Storage blocked", "SecurityError");
        },
      });
    });
    const denied = await blocked.newPage();
    await denied.goto(origin);
    await denied.getByRole("combobox").selectOption("es");
    await expect(denied.getByRole("status")).toHaveText(
      "Se cambió el idioma en esta página. El navegador no pudo recordarlo.",
    );
    await expect(denied.locator("html")).toHaveAttribute("lang", "es");
    assert.deepEqual(errors, []);
  } finally {
    await spanish.close();
    await english.close();
    await blocked.close();
  }
  console.log(
    "English/Spanish static login, keyboard, errors, storage isolation and unchanged URLs passed.",
  );
}
