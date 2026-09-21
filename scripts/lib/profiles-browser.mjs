import assert from "node:assert/strict";
import { resolve } from "node:path";
export async function verifyProfiles(page, origin, principal, runSql) {
  await page.goto(`${origin}/account/profile`);
  await page.getByRole("button", { name: "Edit profile", exact: true }).click();
  await page.getByLabel("Second name (optional)").fill("María");
  await page.getByLabel("Second last name (optional)").fill("Guzmán");
  await page
    .getByLabel("Country (optional)", { exact: true })
    .selectOption("CR");
  await page
    .getByLabel("Calling code (optional)", { exact: true })
    .selectOption("506");
  await page
    .getByLabel("National phone number (optional)", { exact: true })
    .fill("88887777");
  await page.getByLabel("Bio (optional)").fill("🦀".repeat(2000));
  await page.getByRole("button", { name: "Save profile", exact: true }).click();
  await page.getByText("Profile saved.", { exact: true }).waitFor();
  assert.equal(await page.locator(".bio dd").textContent(), "🦀".repeat(2000));
  const original = await page.evaluate(() => {
    const c = document.createElement("canvas");
    c.width = 600;
    c.height = 300;
    const ctx = c.getContext("2d");
    ctx.fillStyle = "#73bb92";
    ctx.fillRect(0, 0, 600, 300);
    return c.toDataURL("image/png").split(",")[1];
  });
  const png = Buffer.from(original, "base64");
  await page
    .getByRole("button", { name: "Change picture", exact: true })
    .click();
  await page.getByLabel("Choose image", { exact: true }).setInputFiles({
    name: "portrait.png",
    mimeType: "image/png",
    buffer: png,
  });
  await page.getByRole("button", { name: "Upload image", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(() => {
    const image = document.querySelector(".avatar");
    return image?.complete && image.naturalWidth === 512;
  });
  const denied = await page.evaluate(async () => {
    const r = await fetch("/api/profiles/me/picture", {
      method: "DELETE",
      headers: { "x-darkhorse-revision": "0" },
    });
    return r.status;
  });
  assert.equal(denied, 403);
  await page.evaluate(() => scrollTo(0, 0));
  await page.screenshot({ path: resolve(".local/profile-desktop.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => scrollTo(0, 0));
  await page.screenshot({ path: resolve(".local/profile-mobile.png") });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    true,
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`${origin}/console/settings`);
  await page.getByRole("button", { name: "Change logo", exact: true }).click();
  await page
    .getByLabel("Choose image", { exact: true })
    .setInputFiles({ name: "logo.png", mimeType: "image/png", buffer: png });
  await page.getByRole("button", { name: "Upload image", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page
    .getByRole("button", { name: "Change background", exact: true })
    .click();
  await page.getByLabel("Choose image", { exact: true }).setInputFiles({
    name: "background.png",
    mimeType: "image/png",
    buffer: png,
  });
  await page.getByRole("button", { name: "Upload image", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.goto(origin);
  await page.waitForFunction(() => {
    const image = document.querySelector('img[src="/api/branding/background"]');
    return image?.complete && image.naturalWidth > 0;
  });
  await page.screenshot({ path: resolve(".local/branding-desktop.png") });
  await page.waitForFunction(() => {
    const image = document.querySelector('img[src="/api/branding/logo"]');
    return image?.complete && image.naturalWidth > 0;
  });
  await page.goto(`${origin}/console/settings`);
  await page.getByRole("button", { name: "Change logo", exact: true }).click();
  await page.getByRole("button", { name: "Remove image", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page
    .getByRole("button", { name: "Change background", exact: true })
    .click();
  await page.getByRole("button", { name: "Remove image", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  const publicState = await page.evaluate(async () =>
    (await fetch("/api/branding")).json(),
  );
  assert.deepEqual(publicState, { logo: false, background: false });
  const audit = await runSql(
    "SELECT count(*) FROM media_audit WHERE event='published'",
  );
  assert.match(audit.stdout, /^\s*3\s*$/m);
  const id = await page.evaluate(async () => {
    const p = await (await fetch("/api/profiles/me")).json();
    return p.id;
  });
  assert.equal(id, principal);
  console.log(
    "Extended Unicode profiles, protected image upload, S3 publication, branding and mobile checks passed.",
  );
}
