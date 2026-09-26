import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_CATALOG_BYTES,
  readBoundedCatalog,
} from "../../lib/i18n-catalog.mjs";

function fileSystem(metadata, bytes) {
  return {
    lstat: async () => metadata,
    readFile: async () => bytes,
  };
}

test("reads bounded UTF-8 bytes only from a regular source file", async () => {
  const bytes = new TextEncoder().encode('["key","value"]');
  assert.equal(
    await readBoundedCatalog(
      "catalog.json",
      fileSystem({ size: bytes.length, isFile: () => true }, bytes),
    ),
    '["key","value"]',
  );
});

test("rejects links and oversized files before reading their contents", async () => {
  let reads = 0;
  const noRead = {
    lstat: async () => ({ size: 1, isFile: () => false }),
    readFile: async () => {
      reads++;
      return new Uint8Array();
    },
  };
  await assert.rejects(
    readBoundedCatalog("catalog.json", noRead),
    /regular file/,
  );
  assert.equal(reads, 0);

  await assert.rejects(
    readBoundedCatalog(
      "catalog.json",
      fileSystem(
        { size: MAX_CATALOG_BYTES + 1, isFile: () => true },
        new Uint8Array(),
      ),
    ),
    /size limit/,
  );
});

test("enforces the byte limit after reading and rejects malformed UTF-8", async () => {
  const file = (bytes) => fileSystem({ size: 0, isFile: () => true }, bytes);
  await assert.rejects(
    readBoundedCatalog(
      "catalog.json",
      file(new Uint8Array(MAX_CATALOG_BYTES + 1)),
    ),
    /size limit/,
  );
  await assert.rejects(
    readBoundedCatalog("catalog.json", file(new Uint8Array([0xff]))),
    /UTF-8/,
  );
});
