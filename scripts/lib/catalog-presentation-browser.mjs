import assert from "node:assert/strict";
import { expect } from "@playwright/test";
import { resolve } from "node:path";

async function fits(dialog) {
  assert.ok(
    await dialog.evaluate((node) => {
      const bounds = node.getBoundingClientRect();
      return (
        bounds.left >= 0 &&
        bounds.right <= window.innerWidth &&
        node.scrollWidth <= node.clientWidth
      );
    }),
    "Catalog dialog must fit the viewport without horizontal overflow",
  );
}

export async function verifyApplicationOverview(
  page,
  application,
  locale = "en",
) {
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  assert.ok((await dialog.boundingBox()).width >= 800);
  for (const [path, en, es] of [
    ["clients", "Clients", "Clientes"],
    ["resources", "Resources", "Recursos"],
    ["scopes", "Scopes", "Ámbitos"],
    ["roles", "Roles", "Roles"],
    ["capabilities", "Capabilities", "Capacidades"],
  ]) {
    const link = dialog.getByRole("link", {
      name: locale === "es" ? es : en,
      exact: true,
    });
    assert.equal(
      await link.getAttribute("href"),
      `/console/${path}?application_id=${application}`,
    );
    assert.ok(await link.getAttribute("aria-describedby"));
  }
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-overview-desktop.png"),
  });
  await dialog
    .getByRole("button", {
      name: locale === "es" ? "Cerrar diálogo" : "Close dialog",
      exact: true,
    })
    .press("Tab");
  assert.equal(
    await page.evaluate(() => document.activeElement?.getAttribute("href")),
    `/console/clients?application_id=${application}`,
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-overview-mobile.png"),
  });
  await dialog
    .getByRole("link", {
      name: locale === "es" ? "Capacidades" : "Capabilities",
      exact: true,
    })
    .focus();
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-overview-mobile-options.png"),
  });
  await page.keyboard.press("Escape");
  await page.locator("dialog").waitFor({ state: "detached" });
  await expect(
    page.getByRole("button", {
      name: locale === "es" ? "Ver Console portal" : "View Console portal",
      exact: true,
    }),
  ).toBeFocused();
  await page.setViewportSize({ width: 1280, height: 900 });
  await page
    .getByRole("button", {
      name: locale === "es" ? "Ver Console portal" : "View Console portal",
      exact: true,
    })
    .click();
}

export async function verifyClientFormGuidance(page, locale = "en") {
  const dialog = page.getByRole("dialog");
  for (const [en, es] of [
    ["Name", "Nombre"],
    ["Active", "Activo"],
    ["Callback URLs", "URLs de retorno"],
    ["Allow refresh tokens", "Permitir tokens de renovación"],
  ]) {
    const label = locale === "es" ? es : en;
    const input = dialog.getByLabel(label, { exact: true });
    const description = await input.getAttribute("aria-describedby");
    assert.ok(description);
    assert.ok(
      (await page.locator(`#${description}`).textContent()).trim().length > 30,
    );
  }
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-client-form-desktop.png"),
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-client-form-mobile.png"),
  });
  await page.setViewportSize({ width: 1280, height: 900 });
}
