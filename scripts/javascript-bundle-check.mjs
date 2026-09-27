import { lstat, readdir, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { relative, resolve, sep } from "node:path";
import { javascriptBundleBudget } from "./lib/javascript-bundle-budget.mjs";

const BUILD_ROOT = resolve("apps/console/build");
const EVIDENCE_PATH = resolve(
  "docs/measurements/localization-quality-gates-2026-09-26.json",
);
const MAX_JAVASCRIPT_FILES = 512;
const MAX_JAVASCRIPT_FILE_BYTES = 32 * 1024 * 1024;

const evidence = JSON.parse(await readFile(EVIDENCE_PATH, "utf8"));
const maximumGzipBytes = evidence?.productionBuildCeiling?.maximumGzipBytes;
const buildAssets = await measureJavaScriptAssets(BUILD_ROOT);
const budget = javascriptBundleBudget({
  maximumGzipBytes,
  currentGzipBytes: buildAssets.gzipBytes,
});

console.log(
  `Production JavaScript: ${buildAssets.fileCount} files, ${buildAssets.rawBytes} raw / ${buildAssets.gzipBytes} gzip bytes; ceiling ${maximumGzipBytes} (${budget.remainingGzipBytes} remaining); SHA-256 ${buildAssets.sha256}.`,
);

if (!budget.withinBudget) {
  throw new Error(
    "Production JavaScript exceeds the recorded bundle-size ceiling. Review the route-level change and update the measurement only with comparable evidence.",
  );
}

async function measureJavaScriptAssets(root) {
  let rootMetadata;
  try {
    rootMetadata = await lstat(root);
  } catch {
    throw new Error(
      "Production web build is missing; run make build-web first.",
    );
  }
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("Production web build must be a regular directory.");
  }

  const rootPrefix = `${root}${sep}`;
  const stack = [root];
  const measurements = [];
  let rawBytes = 0;
  let gzipBytes = 0;

  while (stack.length > 0) {
    const directory = stack.pop();
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      if (!path.startsWith(rootPrefix))
        throw new Error("Production web build contains an invalid path.");
      if (entry.isSymbolicLink())
        throw new Error("Production web build contains a symbolic link.");
      if (entry.isDirectory()) {
        stack.push(path);
        continue;
      }
      if (!entry.isFile() || !entry.name.endsWith(".js")) continue;
      if (measurements.length >= MAX_JAVASCRIPT_FILES) {
        throw new Error("Production JavaScript file-count limit exceeded.");
      }

      let metadata;
      try {
        metadata = await lstat(path);
      } catch {
        throw new Error("Cannot inspect a production JavaScript asset.");
      }
      if (
        !metadata.isFile() ||
        metadata.size < 0 ||
        metadata.size > MAX_JAVASCRIPT_FILE_BYTES
      ) {
        throw new Error("Production JavaScript asset size is invalid.");
      }

      let contents;
      try {
        contents = await readFile(path);
      } catch {
        throw new Error("Cannot read a production JavaScript asset.");
      }
      if (
        contents.byteLength !== metadata.size ||
        contents.byteLength > MAX_JAVASCRIPT_FILE_BYTES
      ) {
        throw new Error("Production JavaScript asset changed while measured.");
      }

      const measured = {
        path: relative(root, path).split(sep).join("/"),
        contents,
        rawBytes: contents.byteLength,
        gzipBytes: gzipSync(contents).byteLength,
      };
      measurements.push(measured);
      rawBytes += measured.rawBytes;
      gzipBytes += measured.gzipBytes;
      if (!Number.isSafeInteger(rawBytes) || !Number.isSafeInteger(gzipBytes)) {
        throw new Error("Production JavaScript total size is invalid.");
      }
    }
  }

  if (measurements.length === 0) {
    throw new Error("Production web build contains no JavaScript assets.");
  }

  const hash = createHash("sha256");
  for (const measurement of measurements.sort((a, b) =>
    a.path < b.path ? -1 : a.path > b.path ? 1 : 0,
  )) {
    hash.update(measurement.path).update(measurement.contents);
  }

  return {
    fileCount: measurements.length,
    rawBytes,
    gzipBytes,
    sha256: hash.digest("hex"),
  };
}
