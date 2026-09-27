import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_API_SPEC_BYTES,
  readBoundedApiInput,
  validateApiJsonResponse,
  validateApiReference,
} from "../../lib/api-spec.mjs";

const route = {
  method: "GET",
  path: "/authorize",
  operation_id: "getAuthorization",
  surface: "integration",
};
const formRequestSchema = (required, rejected = []) => ({
  type: "object",
  additionalProperties: true,
  required,
  properties: {},
  not: {
    anyOf: [
      { required: ["client_id"] },
      { required: ["client_secret"] },
      { required: ["client_assertion"] },
      { required: ["client_assertion_type"] },
      ...rejected.map((field) => ({ required: [field] })),
    ],
  },
});
const fixture = (overrides = {}) => ({
  specification: {
    openapi: "3.2.1",
    info: { version: "1.2.3" },
    paths: {
      "/authorize": {
        get: {
          operationId: "getAuthorization",
          summary: "Start or continue authorization.",
          description: "Use a top-level browser navigation.",
          "x-darkhorse-localization": {
            es: {
              summary: "Iniciar o continuar la autorización.",
              description: "Usa la navegación principal del navegador.",
              group: "Autorización",
            },
          },
          responses: { 302: { description: "Redirect" } },
        },
      },
    },
    components: {
      schemas: {
        Authorization: { type: "object" },
        AuthorizationCodeGrant: formRequestSchema(
          ["grant_type", "code"],
          ["refresh_token"],
        ),
        RefreshTokenGrant: formRequestSchema(
          ["grant_type", "refresh_token"],
          ["code", "redirect_uri", "code_verifier"],
        ),
        IntrospectionRequest: {
          ...formRequestSchema(["token"]),
          properties: { token_type_hint: { type: "string" } },
        },
        RevocationRequest: {
          ...formRequestSchema(["token"]),
          properties: { token_type_hint: { type: "string" } },
        },
      },
    },
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

test("validates live JSON responses through bounded local OpenAPI references", () => {
  const specification = {
    paths: {
      "/fixture": {
        get: {
          responses: { 200: { $ref: "#/components/responses/Success" } },
        },
      },
    },
    components: {
      responses: {
        Success: {
          content: {
            "application/json": {
              schema: { $ref: "#/components/schemas/Status" },
            },
          },
        },
      },
      schemas: {
        Status: {
          type: "object",
          required: ["active"],
          properties: { active: { type: "boolean" } },
          additionalProperties: false,
        },
      },
    },
  };
  assert.deepEqual(
    validateApiJsonResponse(specification, "/fixture", "get", 200, {
      active: true,
    }),
    { ok: true },
  );
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "get", 200, {
      active: "yes",
      credential: "do-not-echo",
    }).error,
    /does not match/,
  );
  assert.equal(
    JSON.stringify(
      validateApiJsonResponse(specification, "/fixture", "get", 200, {
        active: "yes",
        credential: "do-not-echo",
      }),
    ).includes("do-not-echo"),
    false,
  );
});

test("fails closed when a live response status, operation or schema is absent", () => {
  const specification = {
    paths: {
      "/fixture": {
        get: { responses: { 200: { description: "No response body" } } },
      },
    },
  };
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "get", 201, {}).error,
    /not documented/,
  );
  assert.match(
    validateApiJsonResponse(specification, "/missing", "get", 200, {}).error,
    /operation is missing/,
  );
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "get", 200, {}).error,
    /does not define JSON schema/,
  );
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "connect", 200, {})
      .error,
    /operation is missing/,
  );
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "get", 99, {}).error,
    /operation is missing/,
  );
});

test("does not follow cyclic response references", () => {
  const specification = {
    paths: {
      "/fixture": {
        get: {
          responses: { 200: { $ref: "#/components/responses/Loop" } },
        },
      },
    },
    components: {
      responses: {
        Loop: { $ref: "#/components/responses/Loop" },
      },
    },
  };
  assert.match(
    validateApiJsonResponse(specification, "/fixture", "get", 200, {}).error,
    /does not define JSON schema/,
  );
});

test("rejects release version drift", () => {
  assert.equal(
    validateApiReference(fixture({ releaseVersion: "1.2.4" })).error,
    "OpenAPI release version does not match the application.",
  );
});

test("requires bounded Spanish operation text for every supported route", () => {
  const missing = fixture();
  delete missing.specification.paths["/authorize"].get[
    "x-darkhorse-localization"
  ];
  assert.match(validateApiReference(missing).error, /Spanish display text/);

  const oversized = fixture();
  oversized.specification.paths["/authorize"].get[
    "x-darkhorse-localization"
  ].es.description = "x".repeat(4097);
  assert.match(validateApiReference(oversized).error, /Spanish display text/);

  const empty = fixture();
  empty.specification.paths["/authorize"].get[
    "x-darkhorse-localization"
  ].es.group = "  ";
  assert.match(validateApiReference(empty).error, /Spanish display text/);
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
    get: {
      operationId: "getExtra",
      summary: "Get an extra value.",
      description: "Read one extra value.",
      "x-darkhorse-localization": {
        es: {
          summary: "Consultar un valor adicional.",
          description: "Lee un valor adicional.",
          group: "Otro",
        },
      },
      responses: { 200: { description: "ok" } },
    },
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

test("requires OAuth form schemas to allow extensions and reject body client credentials", () => {
  const input = fixture();
  input.specification.components.schemas.AuthorizationCodeGrant.additionalProperties = false;
  assert.match(
    validateApiReference(input).error,
    /OAuth form parameter policy/,
  );

  const missingDenial = fixture();
  missingDenial.specification.components.schemas.IntrospectionRequest.not.anyOf =
    [{ required: ["client_secret"] }];
  assert.match(
    validateApiReference(missingDenial).error,
    /OAuth form parameter policy/,
  );

  const mixedGrant = fixture();
  mixedGrant.specification.components.schemas.AuthorizationCodeGrant.not.anyOf =
    mixedGrant.specification.components.schemas.AuthorizationCodeGrant.not.anyOf.filter(
      (entry) => !entry.required.includes("refresh_token"),
    );
  assert.match(
    validateApiReference(mixedGrant).error,
    /OAuth form parameter policy/,
  );

  const valid = fixture();
  assert.equal(validateApiReference(valid).ok, true);
});

test("does not document an unimplemented token_type_hint length restriction", () => {
  const input = fixture();
  input.specification.components.schemas.IntrospectionRequest.properties.token_type_hint.maxLength = 128;
  assert.equal(
    validateApiReference(input).error,
    "Ignored token_type_hint must not claim value constraints the HTTP parser does not enforce.",
  );
});

test("validates source-defined request examples against their local schema", () => {
  const input = fixture();
  input.specification.paths["/authorize"].get.requestBody = {
    content: {
      "application/json": {
        schema: {
          type: "object",
          required: ["name"],
          additionalProperties: false,
          properties: {
            name: { type: "string", minLength: 2 },
            schema: { type: "string" },
          },
        },
        examples: {
          valid: { value: { name: "Ada", schema: "sample payload field" } },
        },
      },
    },
  };
  assert.equal(validateApiReference(input).ok, true);

  input.specification.paths["/authorize"].get.requestBody.content[
    "application/json"
  ].examples.valid.value.name = "A";
  assert.equal(
    validateApiReference(input).error,
    "OpenAPI example does not satisfy its schema.",
  );
});

test("rejects an example that places Basic credentials in an OAuth request body", () => {
  const input = fixture();
  input.specification.paths["/authorize"].get.requestBody = {
    content: {
      "application/x-www-form-urlencoded": {
        schema: { $ref: "#/components/schemas/AuthorizationCodeGrant" },
        examples: {
          code: {
            value: {
              grant_type: "authorization_code",
              code: "synthetic-code",
            },
          },
        },
      },
    },
  };
  assert.equal(validateApiReference(input).ok, true);
  input.specification.paths["/authorize"].get.requestBody.content[
    "application/x-www-form-urlencoded"
  ].examples.code.value.client_secret = "synthetic-secret";
  assert.equal(
    validateApiReference(input).error,
    "OpenAPI example does not satisfy its schema.",
  );
});

test("checks schema references, oneOf, rejected fields and singular parameter examples", () => {
  const input = fixture();
  input.specification.components.schemas.Name = {
    type: "string",
    pattern: "^[A-Z][a-z]+$",
  };
  input.specification.paths["/authorize"].get.parameters = [
    {
      name: "name",
      in: "query",
      schema: { $ref: "#/components/schemas/Name" },
      example: "Ada",
    },
  ];
  assert.equal(validateApiReference(input).ok, true);

  input.specification.paths["/authorize"].get.parameters[0].example = "ada";
  assert.equal(
    validateApiReference(input).error,
    "OpenAPI example does not satisfy its schema.",
  );

  const grant = fixture();
  grant.specification.components.schemas.AuthorizationCodeGrant.properties = {
    grant_type: { const: "authorization_code", type: "string" },
    code: { type: "string" },
  };
  grant.specification.components.schemas.RefreshTokenGrant.properties = {
    grant_type: { const: "refresh_token", type: "string" },
  };
  grant.specification.paths["/authorize"].get.requestBody = {
    content: {
      "application/x-www-form-urlencoded": {
        schema: {
          oneOf: [
            { $ref: "#/components/schemas/AuthorizationCodeGrant" },
            { $ref: "#/components/schemas/RefreshTokenGrant" },
          ],
        },
        examples: {
          authorizationCode: {
            value: {
              grant_type: "authorization_code",
              code: "one-use-code",
            },
          },
        },
      },
    },
  };
  assert.equal(validateApiReference(grant).ok, true);
  grant.specification.paths["/authorize"].get.requestBody.content[
    "application/x-www-form-urlencoded"
  ].examples.authorizationCode.value.refresh_token = "wrong-grant-field";
  assert.equal(
    validateApiReference(grant).error,
    "OpenAPI example does not satisfy its schema.",
  );

  const invalidSchema = fixture();
  invalidSchema.specification.components.schemas.Unsupported = {
    type: "object",
    unevaluatedProperties: false,
  };
  assert.match(
    validateApiReference(invalidSchema).error,
    /unsupported.*schema/i,
  );
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
