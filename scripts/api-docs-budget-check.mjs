import { readdir, readFile, lstat } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { isAbsolute, join, relative, resolve } from "node:path";
import {
  API_DOCS_GZIP_BUDGET_BYTES,
  collectRouteAssets,
  sumAssetSizes,
  withinApiDocsBudget,
} from "./lib/api-docs-budget.mjs";

const generatedNodes =
  "apps/console/.svelte-kit/generated/client-optimized/nodes";
const clientRoot = resolve("apps/console/.svelte-kit/output/client");
const manifestPath = join(clientRoot, ".vite/manifest.json");
const routeMarker = "src/routes/console/api-docs/+page.svelte";

try {
  const nodeFiles = (await readdir(generatedNodes)).filter((name) =>
    /^\d+\.js$/.test(name),
  );
  const routeNodes = [];
  for (const name of nodeFiles) {
    const path = join(generatedNodes, name);
    const metadata = await lstat(path);
    if (!metadata.isFile() || metadata.size > 64 * 1024)
      throw new Error("Generated API documentation route node is invalid.");
    if ((await readFile(path, "utf8")).includes(routeMarker))
      routeNodes.push(name);
  }
  if (routeNodes.length !== 1)
    throw new Error("Cannot identify the API documentation route build entry.");

  const manifestMetadata = await lstat(manifestPath);
  if (!manifestMetadata.isFile() || manifestMetadata.size > 2 * 1024 * 1024)
    throw new Error("Frontend asset manifest is invalid.");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const sourceKey = `.svelte-kit/generated/client-optimized/nodes/${routeNodes[0]}`;
  const entryKey = Object.entries(manifest).find(
    ([, entry]) => entry?.src === sourceKey,
  )?.[0];
  if (!entryKey)
    throw new Error(
      "API documentation route is absent from the asset manifest.",
    );

  const files = collectRouteAssets(manifest, entryKey);
  const measurements = [];
  for (const file of files) {
    if (typeof file !== "string" || isAbsolute(file))
      throw new Error("Frontend manifest contains an invalid asset path.");
    const path = resolve(clientRoot, file);
    const relativePath = relative(clientRoot, path);
    if (relativePath.startsWith("..") || isAbsolute(relativePath))
      throw new Error("Frontend asset escaped the build directory.");
    const metadata = await lstat(path);
    if (!metadata.isFile() || metadata.size > 5 * 1024 * 1024)
      throw new Error("API documentation asset is invalid or oversized.");
    const bytes = await readFile(path);
    measurements.push({
      file,
      bytes: bytes.byteLength,
      gzipBytes: gzipSync(bytes).byteLength,
    });
  }
  const total = sumAssetSizes(measurements);
  if (!withinApiDocsBudget(total))
    throw new Error(
      `API documentation route is ${total.gzipBytes} gzip bytes; the budget is ${API_DOCS_GZIP_BUDGET_BYTES}.`,
    );
  process.stdout.write(
    `API documentation route assets: ${total.bytes} bytes raw, ${total.gzipBytes} bytes gzip across ${files.length} files (budget ${API_DOCS_GZIP_BUDGET_BYTES}).\n`,
  );
} catch (error) {
  process.stderr.write(
    `API documentation asset check failed: ${error instanceof Error ? error.message : "invalid build"}\n`,
  );
  process.exitCode = 1;
}
