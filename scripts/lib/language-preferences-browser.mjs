import assert from "node:assert/strict";
import { expect } from "@playwright/test";

const principal = "00000000-0000-0000-0000-00000000e037";
const credential = "00000000-0000-0000-0000-00000000e038";

async function signIn(page, origin, password) {
  await page.goto(origin);
  await page
    .getByLabel("Email address", { exact: true })
    .fill("language@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("heading", { name: /Language\./ })).toBeVisible();
}

async function choose(page, origin, locale) {
  await page.goto(`${origin}/account/profile`);
  await page
    .getByRole("button", { name: "Change language", exact: true })
    .click();
  await page
    .getByLabel("Preferred language", { exact: true })
    .selectOption(locale);
  await page
    .getByRole("button", { name: "Save language", exact: true })
    .click();
  await expect(page.getByRole("status")).toHaveText(
    "Language preference saved.",
  );
  await expect(page.locator("dialog")).toHaveCount(0);
}

async function profile(page) {
  return page.evaluate(async () => (await fetch("/api/profiles/me")).json());
}

export async function verifyLanguagePreferences(
  browser,
  origin,
  password,
  source,
  runSql,
) {
  // Isolated fixture identity avoids consuming the administrator's login budget.
  await runSql(`INSERT INTO principals(id,email,first_name,last_name) VALUES('${principal}','language@example.com','Language','Test');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${principal}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${source}' AND NOT c.revoked;`);
  const first = await browser.newContext({ locale: "en-US" });
  const second = await browser.newContext({ locale: "en-US" });
  const fallback = await browser.newContext({ locale: "fr-FR" });
  try {
    const page = await first.newPage();
    const other = await second.newPage();
    const automatic = await fallback.newPage();
    await automatic.goto(origin);
    await expect(automatic.getByRole("combobox")).toHaveValue("es");
    await expect(automatic.locator("html")).toHaveAttribute("lang", "es");
    assert.deepEqual(
      await automatic.evaluate(async () =>
        (await fetch("/api/presentation")).json(),
      ),
      { default_locale: "es" },
    );
    await signIn(page, origin, password);
    const before = await profile(page);
    assert.equal(before.preferred_locale, null);
    await page.getByRole("combobox").selectOption("es");
    assert.equal(
      (await profile(page)).preferred_locale,
      null,
      "presentation choice must not silently write an account preference",
    );
    await choose(page, origin, "es");
    const saved = await profile(page);
    assert.equal(saved.preferred_locale, "es");
    assert.equal(BigInt(saved.revision), BigInt(before.revision) + 1n);
    await signIn(other, origin, password);
    await expect(other.getByRole("combobox")).toHaveValue("es");
    assert.equal(
      await other.evaluate(() => localStorage.length),
      0,
      "saved account preference must not enter anonymous storage",
    );
    await other.getByRole("combobox").selectOption("en");
    await expect(other.locator("html")).toHaveAttribute("lang", "en");
    assert.equal((await profile(other)).preferred_locale, "es");
    await other.reload();
    await expect(other.getByRole("combobox")).toHaveValue("es");
    assert.equal(
      await other.evaluate(() => localStorage.getItem("darkhorse.locale.v1")),
      "en",
    );
    await other
      .getByRole("button", { name: "Cerrar sesión", exact: true })
      .click();
    await expect(
      other.getByRole("button", { name: "Sign in", exact: true }),
    ).toBeVisible();
    await expect(other.getByRole("combobox")).toHaveValue("en");
    await choose(page, origin, "");
    assert.equal((await profile(page)).preferred_locale, null);
    await signIn(other, origin, password);
    await expect(other.getByRole("combobox")).toHaveValue("en");
    const denied = await other.evaluate(async () => {
      const current = await (await fetch("/api/profiles/me")).json();
      const response = await fetch("/api/profiles/me/language", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ revision: current.revision, locale: "es" }),
      });
      return response.status;
    });
    assert.equal(denied, 403);
    assert.equal((await profile(other)).preferred_locale, null);
    await runSql(`DO $$ BEGIN
IF (SELECT credential_epoch FROM principals WHERE id='${principal}') <> 0 THEN RAISE EXCEPTION 'language changed credential epoch'; END IF;
IF (SELECT count(*) FROM profile_audit WHERE target_id='${principal}') <> 2 THEN RAISE EXCEPTION 'unexpected language mutation'; END IF;
END $$;`);
  } finally {
    await first.close();
    await second.close();
    await fallback.close();
  }
  console.log(
    "Saved/cleared account language, cross-browser precedence, private-state logout, CSRF and deployment fallback passed.",
  );
}
