import assert from "node:assert/strict";
import { expect } from "@playwright/test";
import { resolve } from "node:path";
export async function verifyConsoleLanguages(page, origin) {
  const writes = [];
  const onRequest = (request) => {
    if (request.method() !== "GET") writes.push(request.method());
  };
  page.on("request", onRequest);
  try {
    await page.goto(`${origin}/console/settings`);
    await page
      .getByRole("button", { name: "Change logo", exact: true })
      .waitFor();
    await page
      .getByRole("combobox", { name: /^(Language|Idioma)$/ })
      .selectOption("es");
    await page
      .getByRole("heading", {
        name: "Apariencia del inicio de sesión",
        exact: true,
      })
      .waitFor();
    await expect(page.locator("html")).toHaveAttribute("lang", "es");
    await expect(
      page.getByRole("link", { name: "Aplicaciones", exact: true }),
    ).toHaveAttribute("href", "/console/applications");
    await page
      .getByRole("button", { name: "Cambiar logotipo", exact: true })
      .click();
    await expect(
      page.getByRole("dialog", {
        name: "Cambiar logotipo del inicio de sesión",
        exact: true,
      }),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator("dialog")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Cambiar logotipo", exact: true }),
    ).toBeFocused();
    assert.ok(
      await page.evaluate(() => {
        const header = document
          .querySelector(".console-topbar")
          .getBoundingClientRect();
        const language = document
          .querySelector(".console-topbar .language-selector")
          .getBoundingClientRect();
        return language.top >= header.top && language.bottom <= header.bottom;
      }),
      "The header must contain the translated selector and help text",
    );
    await page.screenshot({
      path: resolve(".local/settings-es-desktop.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await page.screenshot({
      path: resolve(".local/settings-es-mobile.png"),
      fullPage: true,
    });
    await page.getByRole("link", { name: "Aplicaciones", exact: true }).click();
    await expect(page.locator(".console-sidebar")).toHaveAttribute(
      "lang",
      "es",
    );
    await expect(page).toHaveURL(`${origin}/console/applications`);
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    assert.deepEqual(
      writes,
      [],
      "Browsing and language selection must not perform mutations",
    );
    await page
      .getByRole("combobox", { name: /^(Language|Idioma)$/ })
      .selectOption("en");
  } finally {
    page.off("request", onRequest);
    await page.setViewportSize({ width: 1280, height: 900 });
  }
  console.log(
    "Console navigation and branding language, literal paths, keyboard, mixed-page language and mobile checks passed.",
  );
}
