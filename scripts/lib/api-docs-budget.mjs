export const API_DOCS_GZIP_BUDGET_BYTES = 80 * 1024;

export function withinApiDocsBudget(total) {
  return (
    isRecord(total) &&
    Number.isSafeInteger(total.gzipBytes) &&
    total.gzipBytes >= 0 &&
    total.gzipBytes <= API_DOCS_GZIP_BUDGET_BYTES
  );
}

/** Collect the route entry, its eagerly imported chunks, and route CSS once. */
export function collectRouteAssets(manifest, entryKey) {
  if (!isRecord(manifest) || typeof entryKey !== "string")
    throw new Error("API documentation build manifest is malformed.");
  const assets = new Set();
  const visited = new Set();
  const visit = (key) => {
    if (visited.has(key)) return;
    visited.add(key);
    const entry = manifest[key];
    if (!isRecord(entry) || typeof entry.file !== "string")
      throw new Error("API documentation build entry is missing.");
    assets.add(entry.file);
    for (const css of entry.css ?? []) {
      if (typeof css !== "string")
        throw new Error("API documentation CSS entry is malformed.");
      assets.add(css);
    }
    for (const imported of entry.imports ?? []) {
      if (typeof imported !== "string")
        throw new Error("API documentation import entry is malformed.");
      visit(imported);
    }
  };
  visit(entryKey);
  return [...assets].sort();
}

export function sumAssetSizes(assets) {
  if (!Array.isArray(assets))
    throw new Error("API documentation asset measurements are malformed.");
  return assets.reduce(
    (total, asset) => {
      if (
        !isRecord(asset) ||
        !Number.isSafeInteger(asset.bytes) ||
        asset.bytes < 0 ||
        !Number.isSafeInteger(asset.gzipBytes) ||
        asset.gzipBytes < 0
      )
        throw new Error("API documentation asset measurement is malformed.");
      return {
        bytes: total.bytes + asset.bytes,
        gzipBytes: total.gzipBytes + asset.gzipBytes,
      };
    },
    { bytes: 0, gzipBytes: 0 },
  );
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
