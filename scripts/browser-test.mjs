import assert from "node:assert/strict";
import { createHash, X509Certificate, randomBytes } from "node:crypto";
import { createServer } from "node:net";
import { request } from "node:https";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium } from "@playwright/test";
import { tlsProxy } from "./lib/redis-test-proxy.mjs";
import { runtimeEnvironment } from "./lib/redis-settings.mjs";
import { startProcess } from "./lib/process.mjs";
import { seedSigning, verifyProvider } from "./lib/provider-browser.mjs";
import { verifyRegistration } from "./lib/registration-browser.mjs";

async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}
function https(origin, ca, path, options = {}) {
  return new Promise((resolve, reject) => {
    const req = request(
      `${origin}${path}`,
      { ca, timeout: 3000, ...options },
      (res) => {
        let body = "";
        res.on("data", (chunk) => (body += chunk));
        res.on("end", () =>
          resolve({ status: res.statusCode, body, headers: res.headers }),
        );
      },
    );
    req.on("error", reject);
    req.on("timeout", () => req.destroy(new Error("HTTPS timeout")));
    req.end();
  });
}
async function ready(origin, ca) {
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      if ((await https(origin, ca, "/health/live")).status === 200) return;
    } catch {}
    await delay(100);
  }
  throw new Error("Browser test server did not start.");
}
export async function verifyBrowser(env, db, directory, command, docker) {
  const port = await freePort();
  const tls = await tlsProxy(port, directory, command);
  const origin = `https://localhost:${tls.port}`;
  const url = new URL(env.DARKHORSE_DATABASE_URL);
  url.pathname = "/browser_test";
  const runtime = {
    ...runtimeEnvironment(env),
    DARKHORSE_DATABASE_URL: url.href,
    DARKHORSE_HTTP_PORT: String(port),
    DARKHORSE_PUBLIC_ORIGIN: origin,
    DARKHORSE_LOGIN_ENABLED: "true",
    DARKHORSE_PROVIDER_ENABLED: "true",
    DARKHORSE_SIGNING_WRAP_KEY: randomBytes(32).toString("hex"),
    DARKHORSE_LOGIN_LIMIT_KEY: randomBytes(32).toString("hex"),
  };
  const executable = resolve(
    process.env.CARGO_TARGET_DIR ?? "target",
    "debug/darkhorse-server",
  );
  const invoke = async (args, input, operator = false) => {
    const result = await command(executable, args, {
      env: {
        ...runtime,
        ...(operator
          ? {
              DARKHORSE_REDIS_LIMITER_ADMIN_URL:
                env.DARKHORSE_REDIS_LIMITER_ADMIN_URL,
            }
          : {}),
      },
      input,
      capture: true,
      acceptFailure: true,
    });
    if (result.code !== 0)
      throw new Error(
        `Browser fixture ${args[0]} failed: ${result.stderr.trim()}`,
      );
    return result;
  };
  let server, browser;
  try {
    const { password, principal } = await seedDatabase(db, invoke, docker);
    await seedSigning(invoke, docker, db);
    server = startProcess({ command: executable, args: [], env: runtime });
    await ready(origin, tls.ca);
    await assert.rejects(https(origin, undefined, "/health/live"));
    browser = await launchBrowser(directory);
    await exerciseBrowser(browser, origin, tls.ca, password, principal, invoke);
    console.log(
      "HTTPS Chromium login, rotation, reload, cookies, CSRF, logout, revocation and limiter failure checks passed.",
    );
  } finally {
    await browser?.close();
    await server?.stop();
    await tls.close();
  }
}

async function seedDatabase(db, invoke, docker) {
  await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-v",
    "ON_ERROR_STOP=1",
    "-c",
    "CREATE DATABASE browser_test",
  ]);
  await invoke(["migrate"]);
  const authority = await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-tAc",
    "SELECT epoch FROM limiter_authority WHERE singleton",
  ]);
  const epoch = authority.stdout.trim();
  assert.match(epoch, /^[0-9]{1,15}$/);
  const password = randomBytes(24).toString("base64url");
  const bootstrap = await invoke(
    ["bootstrap", "--stdin"],
    JSON.stringify({
      email: "browser@example.com",
      first_name: "Browser",
      last_name: "Test",
      password,
    }),
  );
  const principal = bootstrap.stdout.match(
    /Administrator initialized: ([a-f0-9-]{36})/,
  )[1];
  await invoke(["limiter-fence"]);
  // This database shares the suite's disposable Redis instance. Begin after
  // its previous generation; only fixture owner SQL can seed this test state.
  await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-d",
    "browser_test",
    "-v",
    "ON_ERROR_STOP=1",
    "-c",
    `ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET epoch=${BigInt(epoch) + 1n}; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;`,
  ]);

  // Advance only this newly created disposable database's recovery fixture.
  await docker([
    "exec",
    db.name,
    "psql",
    "-U",
    "postgres",
    "-d",
    "browser_test",
    "-v",
    "ON_ERROR_STOP=1",
    "-c",
    "ALTER TABLE limiter_authority DISABLE TRIGGER limiter_authority_transition; UPDATE limiter_authority SET not_before_ms=0; ALTER TABLE limiter_authority ENABLE TRIGGER limiter_authority_transition;",
  ]);
  await invoke(["limiter-activate"], undefined, true);
  return { password, principal };
}

async function launchBrowser(directory) {
  const cert = new X509Certificate(
    await readFile(join(directory, "server.pem")),
  );
  const spki = createHash("sha256")
    .update(cert.publicKey.export({ type: "spki", format: "der" }))
    .digest("base64");
  // Trust only this disposable leaf public key in this browser process. The
  // independent HTTPS probe above verifies its CA chain and localhost name.
  return chromium.launch({
    args: [`--ignore-certificate-errors-spki-list=${spki}`],
  });
}

async function exerciseBrowser(
  browser,
  origin,
  ca,
  password,
  principal,
  invoke,
) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", () => errors.push("page error"));
  await page.addInitScript(() => {
    window.securityViolations = [];
    document.addEventListener("securitypolicyviolation", (e) =>
      window.securityViolations.push(e.violatedDirective),
    );
  });
  const initial = await verifySignIn(page, context, origin, password);
  await verifyRotation(page, context, origin, ca, password, initial);
  await verifyRegistration(page, principal);
  await verifyProvider(page, context, origin, principal);
  await verifyLogoutAndRevocation(page, context, password, principal, invoke);
  assert.deepEqual(errors, []);
  assert.deepEqual(await page.evaluate(() => window.securityViolations), []);
  assert.equal(page.url(), `${origin}/`);
  await page.screenshot({
    path: resolve(".local/login-browser.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  );
}

async function verifySignIn(page, context, origin, password) {
  await page.goto(origin);
  await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill("wrong password");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Unable to sign in" })
    .waitFor();
  assert.equal(
    await page.getByLabel("Password", { exact: true }).inputValue(),
    "",
  );
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  const initial = (await context.cookies()).find(
    (c) => c.name === "__Host-darkhorse",
  );
  assert.ok(
    initial?.secure &&
      initial.httpOnly &&
      initial.sameSite === "Lax" &&
      initial.path === "/" &&
      initial.domain === "localhost",
  );
  assert.ok(
    !(await page.evaluate(() => document.cookie)).includes("__Host-darkhorse"),
  );
  assert.deepEqual(
    await page.evaluate(() => [localStorage.length, sessionStorage.length]),
    [0, 0],
  );
  await page.reload();
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  assert.equal(
    await page.evaluate(
      async () => (await fetch("/api/auth/logout", { method: "POST" })).status,
    ),
    403,
  );
  return initial;
}

async function verifyRotation(page, context, origin, ca, password, initial) {
  const rotated = await page.evaluate(
    async ({ password }) =>
      (
        await fetch("/api/auth/login", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-darkhorse-csrf": "1",
          },
          body: JSON.stringify({ email: "browser@example.com", password }),
        })
      ).status,
    { password },
  );
  assert.equal(rotated, 200);
  const fresh = (await context.cookies()).find(
    (c) => c.name === "__Host-darkhorse",
  );
  assert.ok(fresh.value !== initial.value);
  assert.equal(
    (
      await https(origin, ca, "/api/auth/session", {
        headers: { cookie: `__Host-darkhorse=${initial.value}` },
      })
    ).status,
    401,
  );
  assert.equal(
    (
      await https(origin, ca, "/api/auth/logout", {
        method: "POST",
        headers: {
          origin: "https://evil.example",
          "x-darkhorse-csrf": "1",
          cookie: `__Host-darkhorse=${fresh.value}`,
        },
      })
    ).status,
    403,
  );
}

async function verifyLogoutAndRevocation(
  page,
  context,
  password,
  principal,
  invoke,
) {
  await page.getByRole("button", { name: "Sign out" }).click();
  await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
  assert.ok(
    !(await context.cookies()).some((c) => c.name === "__Host-darkhorse"),
  );
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  await invoke(["revoke-all", principal, "0"]);
  await page.reload();
  await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
  await invoke(["limiter-fence"]);
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "temporarily unavailable" })
    .waitFor();
}
