import assert from "node:assert/strict";
import { resolve } from "node:path";

export async function verifyAccountOverview(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.getByRole("navigation", { name: "Account tools" }).waitFor();
  assert.equal(await page.title(), "Your account — Darkhorse");
  for (const [name, href] of [
    ["Open console", "/console/users"],
    ["My profile", "/account/profile"],
    ["Manage sessions", "/security/sessions"],
    ["Manage API keys", "/security/keys"],
    ["Verify email", "/security/email"],
  ]) {
    assert.equal(
      await page.getByRole("link", { name }).getAttribute("href"),
      href,
    );
  }
  assert.ok(
    await page
      .getByRole("region", { name: "Welcome, Browser." })
      .evaluate((element) => element.getBoundingClientRect().width > 900),
    "the account overview uses desktop space beyond the sign-in card",
  );
  await page.getByRole("link", { name: "Open console" }).focus();
  await page.keyboard.press("Tab");
  assert.ok(
    await page
      .getByRole("link", { name: "My profile" })
      .evaluate((element) => element === document.activeElement),
  );
  await page.screenshot({
    path: resolve(".local/account-desktop.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
    "account navigation fits the mobile viewport without horizontal scrolling",
  );
  await page.screenshot({
    path: resolve(".local/account-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
}
