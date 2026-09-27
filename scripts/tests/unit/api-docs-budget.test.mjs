import assert from "node:assert/strict";
import test from "node:test";
import {
  API_DOCS_GZIP_BUDGET_BYTES,
  collectRouteAssets,
  sumAssetSizes,
  withinApiDocsBudget,
} from "../../lib/api-docs-budget.mjs";

test("collects each eager route dependency and stylesheet once", () => {
  const manifest = {
    page: {
      file: "page.js",
      imports: ["shared", "shared"],
      css: ["page.css"],
    },
    shared: { file: "shared.js", imports: ["page"], css: ["shared.css"] },
  };
  assert.deepEqual(collectRouteAssets(manifest, "page"), [
    "page.css",
    "page.js",
    "shared.css",
    "shared.js",
  ]);
  assert.deepEqual(
    sumAssetSizes([
      { bytes: 100, gzipBytes: 40 },
      { bytes: 50, gzipBytes: 20 },
    ]),
    { bytes: 150, gzipBytes: 60 },
  );
});

test("fails closed for incomplete manifests and invalid measurements", () => {
  assert.throws(() => collectRouteAssets({}, "missing"), /entry is missing/);
  assert.throws(
    () => sumAssetSizes([{ bytes: -1, gzipBytes: 2 }]),
    /malformed/,
  );
});

test("keeps the documentation route under its checked-in compressed asset budget", () => {
  assert.equal(API_DOCS_GZIP_BUDGET_BYTES, 80 * 1024);
  assert.equal(withinApiDocsBudget({ gzipBytes: 80 * 1024 }), true);
  assert.equal(withinApiDocsBudget({ gzipBytes: 80 * 1024 + 1 }), false);
  assert.equal(withinApiDocsBudget({ gzipBytes: "80 KiB" }), false);
});
