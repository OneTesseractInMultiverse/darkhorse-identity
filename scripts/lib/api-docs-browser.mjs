import assert from "node:assert/strict";
import { request } from "node:https";
import { resolve } from "node:path";
import { expect } from "@playwright/test";

function readOpenApi(origin, ca) {
  return new Promise((resolveDocument, reject) => {
    const response = request(
      `${origin}/reference/openapi-v1.json`,
      { ca, headers: { accept: "application/json" }, timeout: 3000 },
      (result) => {
        const chunks = [];
        let size = 0;
        result.on("data", (chunk) => {
          size += chunk.length;
          if (size > 128 * 1024) {
            result.destroy(new Error("OpenAPI response exceeded its bound."));
            return;
          }
          chunks.push(chunk);
        });
        result.on("error", () =>
          reject(new Error("OpenAPI response could not be read.")),
        );
        result.on("end", () => {
          if (result.statusCode !== 200) {
            reject(new Error("OpenAPI static route did not return HTTP 200."));
            return;
          }
          try {
            resolveDocument(JSON.parse(Buffer.concat(chunks).toString("utf8")));
          } catch {
            reject(
              new Error("OpenAPI static route did not return valid JSON."),
            );
          }
        });
      },
    );
    response.on("error", () =>
      reject(new Error("OpenAPI static route failed verified HTTPS.")),
    );
    response.on("timeout", () => response.destroy());
    response.end();
  });
}

export async function verifyApiDocumentation(browser, origin, ca) {
  const originalDocument = await readOpenApi(origin, ca);
  assert.equal(originalDocument.info?.version, "0.1.0");
  const context = await browser.newContext({
    locale: "es-CR",
    viewport: { width: 1440, height: 1000 },
    acceptDownloads: true,
  });
  const page = await context.newPage();
  const errors = [];
  const apiCalls = [];
  const referenceCookies = [];
  const headerChecks = [];
  let sessionCookieSeen = false;
  let sessionRejected = false;
  const injectedText = '<img src=x onerror="alert(1)">';

  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await context.addCookies([
    {
      name: "reference_probe",
      value: "synthetic-cookie",
      url: origin,
      httpOnly: true,
      secure: true,
      sameSite: "Lax",
    },
  ]);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.origin !== origin) return;
    if (url.pathname.startsWith("/api/")) apiCalls.push(url.pathname);
    if (
      url.pathname === "/api/auth/session" ||
      url.pathname.startsWith("/reference/")
    ) {
      headerChecks.push(
        request.allHeaders().then((headers) => {
          if (url.pathname === "/api/auth/session")
            sessionCookieSeen = headers.cookie?.includes(
              "reference_probe=synthetic-cookie",
            );
          if (url.pathname.startsWith("/reference/"))
            referenceCookies.push(headers.cookie);
        }),
      );
    }
  });
  page.on("response", (response) => {
    if (
      response.url() === `${origin}/api/auth/session` &&
      response.status() === 401
    )
      sessionRejected = true;
  });
  await page.route("**/reference/openapi-v1.json", async (route) => {
    const document = structuredClone(originalDocument);
    document.paths["/userinfo"].get.summary = injectedText;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(document),
    });
  });

  try {
    await page.goto(`${origin}/console/api-docs#operation-postToken`);
    await expect(
      page.getByRole("heading", {
        name: "Guía para una aplicación web confidencial",
      }),
    ).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("lang", "es");
    await expect(page.locator("#operation-postToken")).toHaveAttribute(
      "open",
      "",
    );
    await expect(page.locator(".api-operation-row")).toHaveCount(7);

    await page.reload();
    await expect(page.locator("#operation-postToken")).toHaveAttribute(
      "open",
      "",
    );

    const search = page.getByRole("searchbox", {
      name: "Buscar operaciones compatibles",
    });
    await search.fill("introspect");
    await expect(page.locator(".api-operation-row")).toHaveCount(1);
    await expect(page.locator(".api-operation-row")).toContainText(
      "/introspect",
    );
    await search.fill("");

    const disclosure = page.locator(".api-operation summary").first();
    await disclosure.focus();
    await page.keyboard.press("Enter");
    await expect(disclosure.locator("..")).toHaveAttribute("open", "");

    await page.locator("#operation-postToken").evaluate((element) => {
      element.open = true;
    });
    await page
      .getByRole("button", {
        name: "Copiar ejemplo authorizationCode de postToken",
      })
      .click();
    await expect(page.locator(".api-copy-status")).toHaveText(
      "Se copió el ejemplo.",
    );
    const download = page.waitForEvent("download");
    await page
      .getByRole("link", { name: "Descargar especificación OpenAPI" })
      .click();
    assert.equal(
      (await download).suggestedFilename(),
      "darkhorse-openapi.json",
    );

    await expect(page.getByText(injectedText, { exact: true })).toBeVisible();
    await expect(page.locator('img[src="x"]')).toHaveCount(0);

    await page.screenshot({
      path: resolve(".local/api-docs-desktop.png"),
      fullPage: true,
    });
    // A 720 CSS-pixel layout is the available width at 200% zoom on a 1440px display.
    await page.setViewportSize({ width: 720, height: 900 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      "API documentation must fit a 720 CSS-pixel viewport",
    );
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      "API documentation must fit a narrow mobile viewport",
    );
    await page.screenshot({
      path: resolve(".local/api-docs-mobile.png"),
      fullPage: true,
    });

    await Promise.all(headerChecks);
    assert.deepEqual(errors, []);
    assert.equal(sessionRejected, true);
    assert.equal(sessionCookieSeen, true);
    assert.ok(referenceCookies.length >= 2);
    assert.ok(referenceCookies.every((cookie) => cookie === undefined));
    assert.deepEqual(
      [...new Set(apiCalls)].sort(),
      ["/api/auth/session", "/api/presentation"],
      "the static reference must not request protected application APIs",
    );
  } finally {
    await context.close();
  }

  console.log(
    "HTTPS API reference search, deep links, keyboard, clipboard, download, localization, safe rendering, signed-out session and mobile/zoom checks passed.",
  );
}
