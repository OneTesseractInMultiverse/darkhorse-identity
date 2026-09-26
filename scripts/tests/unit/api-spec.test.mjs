import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_API_SPEC_BYTES,
  readBoundedApiInput,
  validateApiReference,
} from "../../lib/api-spec.mjs";

const route = {
  method: "GET",
  path: "/authorize",
  operation_id: "getAuthorization",
  surface: "integration",
};
const fixture = (overrides = {}) => ({
  specification: {
    openapi: "3.2.1",
    info: { version: "1.2.3" },
    paths: {
      "/authorize": {
        get: {
          operationId: "getAuthorization",
          responses: { 302: { description: "Redirect" } },
        },
      },
    },
    components: { schemas: { Authorization: { type: "object" } } },
  },
  classification: { version: "1.2.3", entries: [route] },
  releaseVersion: "1.2.3",
  ...overrides,
});

test("accepts a version-aligned operation set and fully resolved local references", () => {
  const input = fixture();
  input.specification.paths["/authorize"].get.parameters = [
    { $ref: "#/components/schemas/Authorization" },
  ];
  assert.deepEqual(validateApiReference(input), {
    ok: true,
    operationCount: 1,
  });
});

test("rejects release version drift", () => {
  assert.equal(
    validateApiReference(fixture({ releaseVersion: "1.2.4" })).error,
    "OpenAPI release version does not match the application.",
  );
});

test("rejects missing, extra, renamed, duplicate, or malformed operations", () => {
  const missing = fixture();
  missing.specification.paths = {};
  assert.match(validateApiReference(missing).error, /operation set/);
  const renamed = fixture();
  renamed.specification.paths["/authorize"].get.operationId = "getOther";
  assert.match(validateApiReference(renamed).error, /does not match/);
  const extra = fixture();
  extra.specification.paths["/extra"] = {
    get: { operationId: "getExtra", responses: { 200: { description: "ok" } } },
  };
  assert.match(validateApiReference(extra).error, /operation set/);
  const duplicate = fixture();
  duplicate.classification.entries.push({ ...route, path: "/other" });
  assert.match(validateApiReference(duplicate).error, /duplicated/);
  const malformed = fixture();
  delete malformed.specification.paths["/authorize"].get.responses;
  assert.match(validateApiReference(malformed).error, /response/);
});

test("rejects unresolved and remote references without fetching them", () => {
  for (const ref of [
    "#/components/schemas/Missing",
    "https://example.invalid/schema.json",
  ]) {
    const input = fixture();
    input.specification.paths["/authorize"].get.parameters = [{ $ref: ref }];
    assert.match(validateApiReference(input).error, /reference/);
  }
});

test("bounded API inputs reject symlinks, oversized sources, invalid UTF-8 and oversized reads", async () => {
  const read = (metadata, bytes) =>
    readBoundedApiInput("fixture", {
      lstat: async () => metadata,
      readFile: async () => bytes,
    });
  await assert.rejects(
    read({ isFile: () => false, size: 1 }, new Uint8Array()),
    /regular file/,
  );
  await assert.rejects(
    read(
      { isFile: () => true, size: MAX_API_SPEC_BYTES + 1 },
      new Uint8Array(),
    ),
    /size limit/,
  );
  await assert.rejects(
    read(
      { isFile: () => true, size: 1 },
      new Uint8Array(MAX_API_SPEC_BYTES + 1),
    ),
    /size limit/,
  );
  await assert.rejects(
    read({ isFile: () => true, size: 1 }, Uint8Array.of(0xff)),
    /UTF-8/,
  );
});
