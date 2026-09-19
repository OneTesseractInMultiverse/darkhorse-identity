import assert from "node:assert/strict";
import { resolve } from "node:path";
import { request } from "node:https";
import { setTimeout as delay } from "node:timers/promises";

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
        body: await response.json(),
        noStore: response.headers.get("cache-control"),
      };
    },
    { path, body },
  );
}
async function signIn(page, origin, password) {
  await page.goto(origin);
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  const [response] = await Promise.all([
    page.waitForResponse(
      (response) => new URL(response.url()).pathname === "/api/auth/login",
    ),
    page.getByRole("button", { name: "Sign in", exact: true }).click(),
  ]);
  assert.equal(
    response.status(),
    200,
    `Secondary login returned ${response.status()}`,
  );
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
}
export async function verifySessionManagement(
  browser,
  page,
  origin,
  password,
  invoke,
  ca,
) {
  // Let the preceding login scenario's one-minute account budget expire naturally.
  // The production limiter stays enabled for these additional browser sign-ins.
  await delay(60_000);
  const secondary = await browser.newContext();
  try {
    const other = await secondary.newPage();
    const errors = [];
    other.on("pageerror", () => errors.push("secondary page error"));
    await signIn(other, origin, password);
    const target = (await call(other, "/api/security/sessions")).body.current;
    await page.getByRole("link", { name: "Manage sessions" }).click();
    await page.getByRole("table").waitFor();
    assert.equal(page.url(), `${origin}/security/sessions`);
    const list = await call(page, "/api/security/sessions");
    assert.equal(list.status, 200);
    assert.equal(list.noStore, "no-store");
    assert.equal(
      list.body.items.filter((row) => row.status === "active").length,
      2,
    );
    assert.ok(list.body.items.some((row) => row.id === target));
    assert.ok(list.body.current !== target);
    await page.screenshot({
      path: resolve(".local/sessions-desktop.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    );
    await page.screenshot({
      path: resolve(".local/sessions-mobile.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 1280, height: 900 });
    await page
      .getByRole("button", { name: "End session", exact: true })
      .click();
    await page.getByRole("dialog").waitFor();
    await page.screenshot({
      path: resolve(".local/sessions-confirm.png"),
      fullPage: true,
    });
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal((await call(other, "/api/auth/session")).status, 200);
    await page
      .getByRole("button", { name: "End session", exact: true })
      .click();
    await page.getByRole("button", { name: "Confirm end session" }).click();
    await page
      .getByRole("button", { name: "End session", exact: true })
      .waitFor({ state: "detached" });
    assert.equal((await call(other, "/api/auth/session")).status, 401);
    assert.equal((await call(page, "/api/auth/session")).status, 200);
    // Deliver the request, then lose its successful response. The UI must not retry.
    await signIn(other, origin, password);
    await page.getByRole("button", { name: "Refresh sessions" }).click();
    let writes = 0;
    let committed = 0;
    await page.route("**/api/security/sessions/end", async (route) => {
      writes++;
      try {
        const cookie = await route.request().headerValue("cookie");
        const status = await deliveredEnd(
          origin,
          ca,
          cookie,
          route.request().postData(),
        );
        if (status === 200) committed++;
      } catch {
        /* Fixed assertions below retain failure without request headers. */
      } finally {
        await route.abort("failed").catch(() => {});
      }
    });
    await page
      .getByRole("button", { name: "End session", exact: true })
      .click();
    await page.getByRole("button", { name: "Confirm end session" }).click();
    await page
      .getByRole("alert")
      .filter({ hasText: "could not be confirmed" })
      .waitFor();
    assert.equal(writes, 1);
    assert.equal(
      committed,
      1,
      "The discarded response must follow a committed termination",
    );
    assert.equal(
      await page
        .getByRole("button", { name: "End this session", exact: true })
        .isDisabled(),
      true,
    );
    assert.equal(
      await page
        .getByRole("alert")
        .evaluate((node) => node === document.activeElement),
      true,
    );
    await page.unroute("**/api/security/sessions/end");
    await page.getByRole("button", { name: "Refresh sessions" }).click();
    await page
      .getByRole("button", { name: "End session", exact: true })
      .waitFor({ state: "detached" });
    assert.equal((await call(other, "/api/auth/session")).status, 401);
    assert.equal(
      await page
        .getByRole("button", { name: "End this session", exact: true })
        .isEnabled(),
      true,
    );
    // Ending the current session clears both the table and the secure browser cookie.
    await signIn(other, origin, password);
    await other.goto(`${origin}/security/sessions`);
    await other
      .getByRole("button", { name: "End this session", exact: true })
      .click();
    await other.getByRole("button", { name: "Confirm end session" }).click();
    await other.getByRole("link", { name: "Sign in", exact: true }).waitFor();
    assert.equal(await other.getByRole("table").count(), 0);
    assert.ok(
      !(await secondary.cookies()).some((c) => c.name === "__Host-darkhorse"),
    );
    assert.deepEqual(errors, []);
    await page.getByRole("link", { name: "Darkhorse home" }).click();
    await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
    console.log(
      "HTTPS session directory, two-browser termination, uncertain response reconciliation, current-session cookie clearing and mobile bounds passed.",
    );
  } catch (error) {
    try {
      const status = JSON.parse((await invoke(["limiter-status"])).stdout);
      console.error(
        `Session browser failure: limiter=${status.phase}, entries=${status.counter_entries}, original session HTTP=${(await call(page, "/api/auth/session")).status}`,
      );
    } catch {
      console.error("Session browser dependency diagnostics unavailable.");
    }
    throw error;
  } finally {
    await secondary.close();
  }
}

// Forward only to the fixture origin, verifying its private CA and hostname.
// Browser SPKI trust does not configure Node's independent HTTPS client.
async function deliveredEnd(origin, ca, cookie, body) {
  return new Promise((resolve, reject) => {
    const req = request(
      `${origin}/api/security/sessions/end`,
      {
        ca,
        method: "POST",
        timeout: 3000,
        headers: {
          cookie,
          origin,
          "content-type": "application/json",
          "x-darkhorse-csrf": "1",
        },
      },
      (response) => {
        response.resume();
        response.on("end", () => resolve(response.statusCode));
        response.on("error", () =>
          reject(new Error("Session fixture response failed.")),
        );
      },
    );
    req.on("error", () =>
      reject(new Error("Verified HTTPS session fixture failed.")),
    );
    req.on("timeout", () =>
      req.destroy(new Error("Session fixture deadline exceeded.")),
    );
    req.end(body);
  });
}
