import assert from "node:assert/strict";
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

export async function verifyApplicationOverview(page, application) {
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  assert.ok((await dialog.boundingBox()).width >= 800);
  for (const section of [
    "Clients",
    "Resources",
    "Scopes",
    "Roles",
    "Capabilities",
  ]) {
    const link = dialog.getByRole("link", { name: section, exact: true });
    assert.equal(
      await link.getAttribute("href"),
      `/console/${section.toLowerCase()}?application_id=${application}`,
    );
    assert.ok(await link.getAttribute("aria-describedby"));
  }
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-overview-desktop.png"),
  });
  await dialog
    .getByRole("button", { name: "Close dialog", exact: true })
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
  await dialog.getByRole("link", { name: "Capabilities", exact: true }).focus();
  await fits(dialog);
  await page.screenshot({
    path: resolve(".local/catalog-overview-mobile-options.png"),
  });
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached" });
  assert.equal(
    await page.evaluate(() =>
      document.activeElement?.getAttribute("aria-label"),
    ),
    "View Console portal",
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await page
    .getByRole("button", { name: "View Console portal", exact: true })
    .click();
}

export async function verifyClientFormGuidance(page) {
  const dialog = page.getByRole("dialog");
  for (const label of [
    "Name",
    "Active",
    "Callback URLs",
    "Allow refresh tokens",
  ]) {
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
