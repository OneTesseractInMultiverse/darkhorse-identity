import assert from "node:assert/strict";
import { request } from "node:https";
import { resolve } from "node:path";
import { registerResource } from "./resource-fixture.mjs";
import { manage } from "./reference-client.mjs";
async function call(page, path, body) {
  return page.evaluate(
    async ({ path, body }) => {
      const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        cache: "no-store",
        headers: {
          "content-type": "application/json",
          "x-darkhorse-csrf": "1",
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      return {
        status: response.status,
        body: response.status === 204 ? null : await response.json(),
        cache: response.headers.get("cache-control"),
      };
    },
    { path, body },
  );
}
async function create(page, name) {
  await page.getByRole("button", { name: "New API key", exact: true }).click();
  await page.getByLabel("Key name", { exact: true }).fill(name);
  await page.getByLabel("Include Personal worker API", { exact: true }).check();
  await page.getByLabel("Expiration", { exact: true }).selectOption("never");
}
export async function verifyPersonalKeys(page, origin, ca, principal, runSql) {
  const options = { page, origin, ca, principal, runSql, call };
  const first = await registerResource(options, "Personal worker");
  const second = await registerResource(options, "Personal other");
  await page.goto(`${origin}/security/keys`);
  await create(page, "Terminal worker");
  await page.getByLabel("All current permissions", { exact: true }).uncheck();
  await page.getByLabel(new RegExp(`read-${first.read}`)).check();
  await page.getByRole("button", { name: "Create key", exact: true }).click();
  const secret = await page
    .getByLabel("API key secret", { exact: true })
    .inputValue();
  assert.match(secret, /^dk_[0-9a-f]{64}$/);
  const check = (app, key = secret) =>
    manage(
      origin,
      ca,
      "introspect",
      app.credential.introspection_client_id,
      app.credential.secret,
      key,
      "access_token",
    );
  const active = await check(first);
  assert.equal(active.status, 200);
  assert.equal(active.body.active, true);
  assert.deepEqual(active.body.capabilities, [first.read]);
  assert.equal(active.body.exp, undefined);
  assert.equal(active.body.client_id, undefined);
  assert.equal(active.body.credential_type, "personal_key");
  assert.deepEqual((await check(second)).body, { active: false });
  assert.deepEqual(
    (
      await manage(
        origin,
        ca,
        "introspect",
        first.client,
        first.secret,
        secret,
        "access_token",
      )
    ).body,
    { active: false },
  );
  const metadata = await call(page, "/api/security/keys");
  assert.equal(metadata.cache, "no-store");
  assert.ok(!JSON.stringify(metadata.body).includes(secret));
  assert.ok(!JSON.stringify(metadata.body).includes("verifier"));
  await page
    .getByRole("button", { name: "I have saved the key", exact: true })
    .click();
  assert.equal(await page.getByLabel("API key secret").count(), 0);
  assert.ok(
    !JSON.stringify(
      await page.evaluate(() => [
        Object.entries(localStorage),
        Object.entries(sessionStorage),
      ]),
    ).includes(secret),
  );
  await page
    .getByRole("button", { name: "View Terminal worker", exact: true })
    .click();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page.screenshot({
    path: resolve(".local/personal-keys-desktop.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  );
  await page.screenshot({
    path: resolve(".local/personal-keys-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  assert.equal(
    await page.evaluate(
      async () =>
        (
          await fetch("/api/security/keys/revoke", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              key_id: "00000000-0000-0000-0000-000000000001",
            }),
          })
        ).status,
    ),
    403,
  );
  await runSql(
    `DELETE FROM role_capabilities WHERE role_id='${first.role}' AND capability_id='${first.read}';`,
  );
  assert.deepEqual((await check(first)).body, { active: false });
  await runSql(
    `INSERT INTO role_capabilities VALUES('${first.role}','${first.read}');`,
  );
  assert.deepEqual((await check(first)).body.capabilities, [first.read]);
  await page
    .getByRole("button", { name: "Revoke Terminal worker", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm revoke key", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "API key revoked." })
    .waitFor();
  assert.deepEqual((await check(first)).body, { active: false });
  // Discard a real committed response; the browser must not retry the issuance.
  let submissions = 0,
    forwardError;
  await create(page, "Lost response worker");
  await page.route("**/api/security/keys", async (route) => {
    if (route.request().method() !== "POST") {
      await route.continue();
      return;
    }
    submissions += 1;
    try {
      await discard(route.request(), ca);
    } catch (error) {
      forwardError = error;
    }
    await route.abort("failed");
  });
  await page.getByRole("button", { name: "Create key", exact: true }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "could not be confirmed" })
    .waitFor();
  assert.equal(forwardError, undefined);
  assert.equal(submissions, 1);
  assert.ok(
    await page
      .getByRole("button", { name: "New API key", exact: true })
      .isDisabled(),
  );
  await page.unroute("**/api/security/keys");
  await page.getByRole("button", { name: "Refresh keys", exact: true }).click();
  const lost = await call(page, "/api/security/keys");
  assert.equal(
    lost.body.items.filter((k) => k.name === "Lost response worker").length,
    1,
  );
  await page
    .getByRole("button", { name: "Revoke Lost response worker", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm revoke key", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "API key revoked." })
    .waitFor();
  await page.goto(origin);
  console.log(
    "Personal-key HTTPS creation, single reveal, scoped introspection, live reductions, revocation, lost-response and responsive console checks passed.",
  );
}
async function discard(original, ca) {
  const headers = await original.allHeaders();
  return new Promise((resolve, reject) => {
    const req = request(
      original.url(),
      { method: original.method(), headers, ca, timeout: 5000 },
      (response) => {
        response.resume();
        response.on("end", () =>
          response.statusCode === 201
            ? resolve()
            : reject(new Error("Unexpected key issuance outcome")),
        );
        response.on("error", () =>
          reject(new Error("Key response unavailable")),
        );
      },
    );
    req.on("error", () => reject(new Error("Key transport unavailable")));
    req.on("timeout", () => req.destroy(new Error("Key request timed out")));
    req.end(original.postData());
  });
}
