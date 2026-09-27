import assert from "node:assert/strict";
import { lstat, readFile, readdir } from "node:fs/promises";
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
          if (result.headers["cache-control"] !== "no-store") {
            reject(new Error("OpenAPI static route must retain no-store."));
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

function postFormExample(origin, ca, fields) {
  if (
    !fields ||
    typeof fields !== "object" ||
    Object.values(fields).some((value) => typeof value !== "string")
  )
    throw new Error("The token endpoint example has an invalid form shape.");
  const body = new URLSearchParams(fields).toString();
  const fakeClient = "00000000-0000-4000-8000-000000000001";
  // Correctly shaped but unregistered. The adapter must parse the form before
  // storage rejects this synthetic client.
  const fakeSecret = "ab".repeat(32);
  const credentials = Buffer.from(
    `${fakeClient}:${fakeSecret}`,
    "utf8",
  ).toString("base64");
  return new Promise((resolveResponse, reject) => {
    const response = request(
      `${origin}/token`,
      {
        ca,
        method: "POST",
        headers: {
          accept: "application/json",
          authorization: `Basic ${credentials}`,
          "content-length": Buffer.byteLength(body),
          "content-type": "application/x-www-form-urlencoded",
        },
        timeout: 3000,
      },
      (result) => {
        const chunks = [];
        let size = 0;
        result.on("data", (chunk) => {
          size += chunk.length;
          if (size > 16 * 1024) {
            result.destroy(
              new Error("Token example response exceeded its bound."),
            );
            return;
          }
          chunks.push(chunk);
        });
        result.on("error", () =>
          reject(new Error("Token example response could not be read.")),
        );
        result.on("end", () => {
          try {
            resolveResponse({
              status: result.statusCode,
              body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
            });
          } catch {
            reject(new Error("Token example response was not valid JSON."));
          }
        });
      },
    );
    response.on("error", () =>
      reject(new Error("Token example request failed verified HTTPS.")),
    );
    response.on("timeout", () => response.destroy());
    response.end(body);
  });
}

async function findApiDocsRouteModule() {
  const generatedPath = resolve(
    "apps/console/.svelte-kit/generated/client-optimized/nodes",
  );
  const nodeFiles = (await readdir(generatedPath)).filter((name) =>
    /^\d+\.js$/.test(name),
  );
  const routeFiles = [];
  for (const name of nodeFiles) {
    const path = resolve(generatedPath, name);
    const metadata = await lstat(path);
    if (!metadata.isFile() || metadata.size > 64 * 1024) continue;
    if (
      (await readFile(path, "utf8")).includes(
        "src/routes/console/api-docs/+page.svelte",
      )
    )
      routeFiles.push(name);
  }
  if (routeFiles.length !== 1)
    throw new Error("Cannot identify the API documentation route module.");
  const manifestPath = resolve(
    "apps/console/.svelte-kit/output/client/.vite/manifest.json",
  );
  const metadata = await lstat(manifestPath);
  if (!metadata.isFile() || metadata.size > 2 * 1024 * 1024)
    throw new Error("Frontend asset manifest is invalid.");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const sourceKey = `.svelte-kit/generated/client-optimized/nodes/${routeFiles[0]}`;
  const entry = Object.values(manifest).find(
    (candidate) => candidate?.src === sourceKey,
  );
  if (typeof entry?.file !== "string")
    throw new Error(
      "API documentation route module is missing from the asset manifest.",
    );
  return `/${entry.file}`;
}

export async function verifyApiDocumentation(browser, origin, ca) {
  const originalDocument = await readOpenApi(origin, ca);
  const tokenExamples =
    originalDocument.paths?.["/token"]?.post?.requestBody?.content?.[
      "application/x-www-form-urlencoded"
    ]?.examples;
  const formExamples = Object.values(tokenExamples ?? {}).filter(
    (example) => example && typeof example.value === "object",
  );
  assert.ok(
    formExamples.length > 0,
    "OpenAPI must publish token request examples",
  );
  for (const example of formExamples) {
    const response = await postFormExample(origin, ca, example.value);
    assert.equal(response.status, 401);
    assert.deepEqual(response.body, { error: "invalid_client" });
  }
  const withBodySecret = await postFormExample(origin, ca, {
    ...formExamples[0].value,
    client_secret: "synthetic-body-secret-must-be-rejected",
  });
  assert.equal(withBodySecret.status, 400);
  assert.deepEqual(withBodySecret.body, { error: "invalid_request" });
  console.log(
    `Rust token transport parsed ${formExamples.length} documented form example(s) before safely rejecting the synthetic client.`,
  );
  const apiDocsRouteModule = await findApiDocsRouteModule();
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
  const cachePolicies = [];
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
    const url = new URL(response.url());
    if (url.origin === origin && url.pathname.startsWith("/_app/immutable/")) {
      cachePolicies.push({
        path: url.pathname,
        cacheControl: response.headers()["cache-control"],
        status: response.status(),
      });
    }
    if (
      url.origin === origin &&
      (url.pathname === "/console/api-docs" ||
        url.pathname.startsWith("/reference/"))
    ) {
      cachePolicies.push({
        path: url.pathname,
        cacheControl: response.headers()["cache-control"],
        status: response.status(),
      });
    }
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
      headers: { "cache-control": "no-store" },
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
    await expect(page.getByRole("main")).toHaveCount(1);
    await expect(page.locator("h1")).toHaveCount(1);
    const resultStatus = page.locator(".api-section-heading .api-result-count");
    assert.equal(await resultStatus.getAttribute("role"), "status");
    assert.equal(await resultStatus.getAttribute("aria-live"), "polite");
    const unnamedControls = await page
      .locator("a[href], button, input")
      .evaluateAll((elements) =>
        elements
          .filter((element) => {
            const labelledBy = element.getAttribute("aria-labelledby");
            const labelText = labelledBy
              ? labelledBy
                  .split(/\s+/)
                  .map((id) => document.getElementById(id)?.textContent ?? "")
                  .join(" ")
              : "";
            const hasName =
              element.getAttribute("aria-label") ||
              labelText.trim() ||
              element.textContent?.trim() ||
              element.getAttribute("title") ||
              (element instanceof HTMLInputElement &&
                (element.labels?.length || element.type === "hidden"));
            return !hasName;
          })
          .map(
            (element) =>
              `${element.tagName.toLowerCase()}${element.id ? `#${element.id}` : ""}`,
          ),
      );
    assert.deepEqual(
      unnamedControls,
      [],
      "interactive controls need accessible names",
    );

    const pageLoadMetrics = (targetPage) =>
      targetPage.evaluate((apiDocsRouteModule) => {
        const navigation = performance.getEntriesByType("navigation")[0];
        const resources = performance
          .getEntriesByType("resource")
          .filter((entry) => new URL(entry.name).origin === location.origin);
        const bytes = (entries, field) =>
          entries.reduce((total, entry) => total + (entry[field] || 0), 0);
        return {
          apiReferenceReadyMs: Math.round(performance.now()),
          domContentLoadedMs: Math.round(
            navigation?.domContentLoadedEventEnd ?? 0,
          ),
          loadEventMs: Math.round(navigation?.loadEventEnd ?? 0),
          sameOriginTransferBytes: bytes(resources, "transferSize"),
          encodedResourceBytes: bytes(resources, "encodedBodySize"),
          resourceCount: resources.length,
          openApiBytes:
            resources.find((entry) =>
              new URL(entry.name).pathname.endsWith(
                "/reference/openapi-v1.json",
              ),
            )?.encodedBodySize ?? null,
          routeModule:
            resources.find(
              (entry) => new URL(entry.name).pathname === apiDocsRouteModule,
            )?.name ?? null,
        };
      }, apiDocsRouteModule);
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
    await expect(resultStatus).toContainText("1");
    await search.fill("route-does-not-exist");
    await expect(page.locator(".api-operation-row")).toHaveCount(0);
    await expect(resultStatus).toContainText("0");
    await expect(
      page.getByText("Ninguna operación coincide con esta búsqueda."),
    ).toHaveAttribute("role", "status");
    await search.fill("");

    const routeSearch = page.getByRole("searchbox", {
      name: "Buscar rutas registradas",
    });
    const routeStatus = page.locator(".api-boundaries .api-result-count");
    await expect(routeStatus).toHaveAttribute("role", "status");
    await expect(routeStatus).toHaveAttribute("aria-live", "polite");
    await expect(routeStatus).toContainText(
      "Cantidad de rutas registradas: 80",
    );
    await routeSearch.fill("/api/admin/users");
    await expect(page.locator(".api-boundary-entry")).toHaveCount(4);
    await expect(routeStatus).toContainText("Cantidad de rutas registradas: 4");
    await routeSearch.fill("administración");
    await expect(page.locator(".api-boundary-entry")).toHaveCount(47);
    await routeSearch.fill("no-existe-en-la-referencia");
    await expect(page.locator(".api-boundary-entry")).toHaveCount(0);
    await expect(routeStatus).toContainText("Cantidad de rutas registradas: 0");
    await expect(
      page.getByText("Ninguna ruta registrada coincide con esta búsqueda."),
    ).toHaveAttribute("role", "status");
    await routeSearch.fill("");

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
    const observedCachePolicies = cachePolicies;
    const immutableAssets = observedCachePolicies.filter((entry) =>
      entry.path.startsWith("/_app/immutable/"),
    );
    const referenceDocuments = observedCachePolicies.filter((entry) =>
      entry.path.startsWith("/reference/"),
    );
    assert.ok(immutableAssets.length > 0);
    assert.ok(
      immutableAssets.every(
        (entry) =>
          entry.status === 200 &&
          entry.cacheControl === "public, max-age=31536000, immutable",
      ),
    );
    assert.ok(referenceDocuments.length > 0);
    assert.ok(
      referenceDocuments.every((entry) => entry.cacheControl === "no-store"),
    );
    assert.ok(
      observedCachePolicies.some(
        (entry) =>
          entry.path === "/console/api-docs" &&
          entry.cacheControl === "no-store",
      ),
    );

    const measurementContext = await browser.newContext({
      locale: "es-CR",
      viewport: { width: 1440, height: 1000 },
    });
    const measurementPage = await measurementContext.newPage();
    await measurementPage.goto(`${origin}/console/api-docs`);
    await expect(measurementPage.locator(".api-operation-row")).toHaveCount(7);
    const coldLoad = await pageLoadMetrics(measurementPage);
    await measurementPage.reload();
    await expect(measurementPage.locator(".api-operation-row")).toHaveCount(7);
    const warmLoad = await pageLoadMetrics(measurementPage);
    assert.ok(
      warmLoad.sameOriginTransferBytes < coldLoad.sameOriginTransferBytes,
      "immutable asset caching should reduce bytes transferred on a repeat visit",
    );
    await measurementContext.close();
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

    const ordinaryContext = await browser.newContext({
      locale: "es-CR",
      viewport: { width: 1440, height: 1000 },
    });
    const ordinaryPage = await ordinaryContext.newPage();
    const ordinaryPageAssets = [];
    ordinaryPage.on("request", (request) =>
      ordinaryPageAssets.push(request.url()),
    );
    await ordinaryPage.goto(`${origin}/`);
    assert.ok(
      ordinaryPageAssets.every((url) => !url.includes("/reference/")),
      "unrelated pages must not download the API reference data",
    );
    assert.ok(
      coldLoad.routeModule,
      "API documentation route module should load",
    );
    assert.ok(
      ordinaryPageAssets.every(
        (url) => new URL(url).pathname !== apiDocsRouteModule,
      ),
      "unrelated pages must not download the API documentation route module",
    );
    await ordinaryPage.close();
    await ordinaryContext.close();
    console.log(
      `API documentation browser-load samples: ${JSON.stringify({ cold: coldLoad, warm: warmLoad })}`,
    );
  } finally {
    await context.close();
  }

  console.log(
    "HTTPS API reference search, deep links, keyboard, clipboard, download, localization, safe rendering, signed-out session and mobile/zoom checks passed.",
  );
}
