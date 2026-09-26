export const MAX_CATALOG_BYTES = 256 * 1024;

/** Read one fixed build input without following links or allocating an unbounded file. */
export async function readBoundedCatalog(path, { lstat, readFile }) {
  let metadata;
  try {
    metadata = await lstat(path);
  } catch {
    throw new Error("Cannot inspect localization catalog source.");
  }
  if (!metadata.isFile())
    throw new Error("Localization catalog source must be a regular file.");
  if (
    !Number.isSafeInteger(metadata.size) ||
    metadata.size > MAX_CATALOG_BYTES
  ) {
    throw new Error("Localization catalog size limit exceeded.");
  }

  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    throw new Error("Cannot read localization catalog source.");
  }
  if (!(bytes instanceof Uint8Array) || bytes.byteLength > MAX_CATALOG_BYTES) {
    throw new Error("Localization catalog size limit exceeded.");
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("Localization catalog source must be valid UTF-8.");
  }
}
