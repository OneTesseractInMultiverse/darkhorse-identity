// Presentation-only measurement. Authentication responses are fixed source fakes.
// Use test-browser for the real Rust/HTTPS authentication boundary.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, readdir, mkdir, writeFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { cpus, platform, arch } from "node:os";
import { chromium } from "@playwright/test";
const baseline = process.env.I18N_BASELINE;
if (!baseline)
  throw new Error(
    "I18N_BASELINE must name an English-only static build directory.",
  );
const roots = [resolve(baseline), resolve("apps/console/build")];
const types = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".json": "application/json",
};
async function serve(root) {
  const server = createServer((req, res) => {
    void respond(root, req, res);
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}
async function respond(root, req, res) {
  try {
    const path = new URL(req.url, "http://localhost").pathname;
    if (path.startsWith("/api/")) {
      res.writeHead(path === "/api/auth/session" ? 401 : 200, {
        "content-type": "application/json",
        "cache-control": "no-store",
      });
      res.end(
        path === "/api/auth/session"
          ? '{"error":"unauthorized"}'
          : path === "/api/presentation"
            ? '{"default_locale":"en"}'
            : '{"logo":false,"background":false}',
      );
      return;
    }
    const file = resolve(root, `.${path === "/" ? "/index.html" : path}`);
    if (!file.startsWith(root + sep)) throw new Error("Invalid path.");
    const data = await readFile(file);
    const gzip = req.headers["accept-encoding"]?.includes("gzip");
    res.writeHead(200, {
      "content-type": types[extname(file)] ?? "application/octet-stream",
      "cache-control": path.includes("/immutable/")
        ? "public, max-age=3600"
        : "no-store",
      ...(gzip ? { "content-encoding": "gzip" } : {}),
    });
    res.end(gzip ? gzipSync(data) : data);
  } catch {
    res.writeHead(404);
    res.end();
  }
}
async function assets(root) {
  const files = (await readdir(root, { recursive: true }))
    .filter((path) => path.endsWith(".js"))
    .sort();
  const rows = [];
  const hash = createHash("sha256");
  for (const file of files) {
    const data = await readFile(resolve(root, file));
    hash.update(file).update(data);
    rows.push({ bytes: data.length, gzip: gzipSync(data).length });
  }
  return {
    jsFiles: rows.length,
    bytes: rows.reduce((a, r) => a + r.bytes, 0),
    gzip: rows.reduce((a, r) => a + r.gzip, 0),
    sha256: hash.digest("hex"),
  };
}
async function navigation(page, origin, locale) {
  const errors = [];
  const onError = () => errors.push("page error");
  page.on("pageerror", onError);
  try {
    await page.goto(origin);
    await page
      .getByRole("button", {
        name: locale === "es" ? "Iniciar sesión" : "Sign in",
        exact: true,
      })
      .waitFor();
    await page.waitForFunction(
      () => !document.querySelector('button[type="submit"]').disabled,
    );
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    assert.deepEqual(errors, []);
    return await page.evaluate(() => ({
      readyMs: performance.now(),
      transferBytes: performance
        .getEntriesByType("resource")
        .reduce((n, e) => n + e.transferSize, 0),
      requests: performance.getEntriesByType("resource").length,
      language: document.documentElement.lang,
    }));
  } finally {
    page.off("pageerror", onError);
  }
}
const reports = await Promise.all(roots.map(assets));
assert.ok(
  reports[1].gzip - reports[0].gzip <= 36 * 1024,
  "Localization foundation exceeds its provisional gzip budget",
);
const services = [];
let browser;
try {
  for (const root of roots) services.push(await serve(root));
  browser = await chromium.launch();
  const observations = [];
  for (let repeat = 0; repeat < 5; repeat++) {
    for (const [variant, locale] of repeat % 2
      ? [
          [1, "es"],
          [0, "en"],
          [1, "en"],
        ]
      : [
          [0, "en"],
          [1, "en"],
          [1, "es"],
        ]) {
      const context = await browser.newContext({
        locale,
        viewport: { width: 1280, height: 900 },
      });
      try {
        const page = await context.newPage();
        const cold = await navigation(page, services[variant].origin, locale);
        const warm = await navigation(page, services[variant].origin, locale);
        observations.push({
          repeat,
          variant: variant ? "localized" : "baseline",
          locale,
          cold,
          warm,
        });
      } finally {
        await context.close();
      }
    }
  }
  const report = {
    schema: 1,
    date: new Date().toISOString(),
    scope:
      "Presentation only: loopback HTTP, gzip, static production builds, fake anonymous session/branding/presentation responses; no real authentication or server-latency claim.",
    node: process.version,
    browser: browser.version(),
    host: { platform: platform(), arch: arch(), cpu: cpus()[0].model },
    budgetAdditionalGzipBytes: 36 * 1024,
    assets: { baseline: reports[0], localized: reports[1] },
    observations,
  };
  await mkdir(".local/benchmarks", { recursive: true });
  await writeFile(
    ".local/benchmarks/i18n.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(
    "Presentation measurement passed; .local/benchmarks/i18n.json retains cold/warm observations and byte counts.",
  );
} finally {
  await browser?.close();
  for (const service of services) await service.close();
}
