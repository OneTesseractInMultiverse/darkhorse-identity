import { isDeepStrictEqual } from "node:util";

export const MAX_API_SPEC_BYTES = 2 * 1024 * 1024;
const METHODS = new Set(["get", "post", "put", "patch", "delete"]);
const JSON_SCHEMA_TYPES = new Set([
  "array",
  "boolean",
  "integer",
  "null",
  "number",
  "object",
  "string",
]);
const SCHEMA_ASSERTIONS = new Set([
  "$ref",
  "additionalProperties",
  "allOf",
  "anyOf",
  "const",
  "enum",
  "format",
  "items",
  "maximum",
  "maxLength",
  "minimum",
  "minLength",
  "not",
  "oneOf",
  "pattern",
  "properties",
  "required",
  "type",
]);
const SCHEMA_ANNOTATIONS = new Set([
  "$comment",
  "default",
  "deprecated",
  "description",
  "examples",
  "readOnly",
  "title",
  "writeOnly",
]);

/** Validate a reviewed OIDC specification against the source-derived route boundary. */
export function validateApiReference({
  specification,
  classification,
  releaseVersion,
}) {
  const fail = (message) => ({ ok: false, error: message });
  if (!isRecord(specification) || specification.openapi !== "3.2.1")
    return fail("Unsupported or malformed OpenAPI document.");
  if (
    !isRecord(specification.info) ||
    specification.info.version !== releaseVersion ||
    classification?.version !== releaseVersion
  )
    return fail("OpenAPI release version does not match the application.");
  if (
    !isRecord(specification.paths) ||
    !isRecord(classification) ||
    !Array.isArray(classification.entries)
  )
    return fail("API route classification is malformed.");

  const integrations = classification.entries.filter(
    (entry) => entry?.surface === "integration",
  );
  const classified = new Map();
  for (const entry of integrations) {
    const method =
      typeof entry.method === "string" ? entry.method.toLowerCase() : "";
    if (
      !METHODS.has(method) ||
      !validPath(entry.path) ||
      !validOperationId(entry.operation_id)
    )
      return fail(
        "Every integration route must have a valid method, path, and operation ID.",
      );
    const key = `${method} ${entry.path}`;
    if (classified.has(key) || classified.has(entry.operation_id))
      return fail("Integration route or operation ID is duplicated.");
    classified.set(key, entry.operation_id);
    classified.set(entry.operation_id, key);
  }

  const documented = new Map();
  const operationIds = new Set();
  for (const [path, pathItem] of Object.entries(specification.paths)) {
    if (!validPath(path) || !isRecord(pathItem))
      return fail("OpenAPI path item is malformed.");
    for (const [method, operation] of Object.entries(pathItem)) {
      if (!METHODS.has(method)) continue;
      if (
        !isRecord(operation) ||
        !validOperationId(operation.operationId) ||
        !isRecord(operation.responses) ||
        !validSpanishOperation(operation)
      )
        return fail(
          "Every operation needs a unique ID, responses, and bounded Spanish display text.",
        );
      const id = operation.operationId;
      const key = `${method} ${path}`;
      if (operationIds.has(id) || documented.has(key))
        return fail("OpenAPI operation is duplicated.");
      operationIds.add(id);
      documented.set(key, id);
    }
  }
  if (
    classified.size !== integrations.length * 2 ||
    documented.size !== integrations.length
  )
    return fail(
      "OpenAPI operation set does not match classified integration routes.",
    );
  for (const [key, operationId] of documented) {
    if (classified.get(key) !== operationId)
      return fail("OpenAPI operation does not match its classified route.");
  }
  for (const [key, operationId] of classified) {
    if (!key.includes(" ")) continue;
    if (documented.get(key) !== operationId)
      return fail("A classified integration route is missing from OpenAPI.");
  }

  const refError = checkReferences(specification);
  if (refError) return fail(refError);
  const schemaError = checkSchemasAndExamples(specification);
  if (schemaError) return fail(schemaError);
  const parameterError = checkOAuthFormParameterPolicy(specification);
  return parameterError
    ? fail(parameterError)
    : { ok: true, operationCount: documented.size };
}

function validSpanishOperation(operation) {
  if (
    typeof operation.summary !== "string" ||
    operation.summary.length === 0 ||
    operation.summary.length > 256 ||
    typeof operation.description !== "string" ||
    operation.description.length === 0 ||
    operation.description.length > 4096
  )
    return false;
  const translation = operation["x-darkhorse-localization"]?.es;
  return (
    isRecord(translation) &&
    typeof translation.summary === "string" &&
    translation.summary.trim().length > 0 &&
    translation.summary.length <= 256 &&
    typeof translation.description === "string" &&
    translation.description.trim().length > 0 &&
    translation.description.length <= 4096 &&
    typeof translation.group === "string" &&
    translation.group.trim().length > 0 &&
    translation.group.length <= 64
  );
}

/** Validate one observed JSON body against its reviewed local OpenAPI response. */
export function validateApiJsonResponse(
  specification,
  path,
  method,
  status,
  value,
) {
  const fail = (error) => ({ ok: false, error });
  const normalizedMethod =
    typeof method === "string" ? method.toLowerCase() : "";
  if (
    !isRecord(specification) ||
    !validPath(path) ||
    !METHODS.has(normalizedMethod) ||
    !Number.isInteger(status) ||
    status < 100 ||
    status > 599
  )
    return fail("OpenAPI operation is missing.");
  const operation = specification.paths?.[path]?.[normalizedMethod];
  if (!isRecord(operation)) return fail("OpenAPI operation is missing.");
  const declaredResponse = operation.responses?.[String(status)];
  if (declaredResponse === undefined)
    return fail("OpenAPI response status is not documented.");
  const response = resolveLocalReference(declaredResponse, specification);
  const schema = response?.content?.["application/json"]?.schema;
  if (schema === undefined)
    return fail("OpenAPI response does not define JSON schema.");
  if (checkSchemaShape(schema))
    return fail("OpenAPI response schema is malformed.");
  if (!matchesSchema(value, schema, specification))
    return fail("Live JSON response does not match its documented schema.");
  return { ok: true };
}

function checkSchemasAndExamples(specification) {
  const schemaRoots = [
    ...Object.values(specification.components?.schemas ?? {}),
  ];
  const exampleChecks = [];
  let malformedExample = false;
  const walk = (value, key = "") => {
    if (
      key === "schemas" ||
      key === "schema" ||
      key === "example" ||
      key === "examples"
    )
      return;
    if (Array.isArray(value)) {
      for (const item of value) walk(item);
      return;
    }
    if (!isRecord(value)) return;
    if (Object.hasOwn(value, "schema")) {
      schemaRoots.push(value.schema);
      if (Object.hasOwn(value, "example"))
        exampleChecks.push([value.schema, value.example]);
      if (isRecord(value.examples)) {
        for (const example of Object.values(value.examples)) {
          const resolved = resolveExample(example, specification);
          if (!resolved.ok) {
            malformedExample = true;
            return;
          }
          if (resolved.hasValue)
            exampleChecks.push([value.schema, resolved.value]);
        }
      }
    }
    for (const [childKey, child] of Object.entries(value))
      walk(child, childKey);
  };
  walk(specification);
  if (malformedExample)
    return "OpenAPI example reference is malformed or external.";

  for (const schema of schemaRoots) {
    const error = checkSchemaShape(schema);
    if (error) return error;
  }
  if (
    exampleChecks.some(
      ([schema, value]) => !matchesSchema(value, schema, specification),
    )
  )
    return "OpenAPI example does not satisfy its schema.";
  return null;
}

function resolveExample(example, specification, depth = 0) {
  if (depth > 32) return { ok: false };
  if (!isRecord(example)) return { ok: false };
  if (Object.hasOwn(example, "$ref")) {
    if (typeof example.$ref !== "string" || !example.$ref.startsWith("#/"))
      return { ok: false };
    return resolveExample(
      resolvePointer(specification, example.$ref),
      specification,
      depth + 1,
    );
  }
  if (Object.hasOwn(example, "externalValue")) return { ok: false };
  return {
    ok: true,
    hasValue: Object.hasOwn(example, "value"),
    value: example.value,
  };
}

function checkSchemaShape(schema, depth = 0) {
  if (typeof schema === "boolean") return null;
  if (!isRecord(schema) || depth > 64)
    return "OpenAPI schema is malformed or too deeply nested.";
  for (const keyword of Object.keys(schema)) {
    if (
      !SCHEMA_ASSERTIONS.has(keyword) &&
      !SCHEMA_ANNOTATIONS.has(keyword) &&
      !keyword.startsWith("x-")
    )
      return "OpenAPI contains an unsupported JSON Schema keyword.";
  }
  if (Object.hasOwn(schema, "type") && !validSchemaTypes(schema.type))
    return "OpenAPI schema type is malformed.";
  if (
    Object.hasOwn(schema, "required") &&
    (!Array.isArray(schema.required) ||
      schema.required.some((value) => typeof value !== "string"))
  )
    return "OpenAPI schema required list is malformed.";
  if (Object.hasOwn(schema, "properties") && !isRecord(schema.properties))
    return "OpenAPI schema properties are malformed.";
  if (
    Object.hasOwn(schema, "enum") &&
    (!Array.isArray(schema.enum) || schema.enum.length === 0)
  )
    return "OpenAPI schema enum is malformed.";
  if (
    Object.hasOwn(schema, "pattern") &&
    (typeof schema.pattern !== "string" || schema.pattern.length > 256)
  )
    return "OpenAPI schema pattern is malformed.";
  for (const keyword of ["minimum", "maximum", "minLength", "maxLength"]) {
    if (
      Object.hasOwn(schema, keyword) &&
      (typeof schema[keyword] !== "number" ||
        !Number.isFinite(schema[keyword]) ||
        (keyword.endsWith("Length") &&
          (!Number.isInteger(schema[keyword]) || schema[keyword] < 0)))
    )
      return "OpenAPI schema bound is malformed.";
  }
  if (Object.hasOwn(schema, "format") && typeof schema.format !== "string")
    return "OpenAPI schema format is malformed.";
  for (const keyword of ["allOf", "anyOf", "oneOf"]) {
    if (
      Object.hasOwn(schema, keyword) &&
      (!Array.isArray(schema[keyword]) ||
        schema[keyword].length === 0 ||
        schema[keyword].some((part) => checkSchemaShape(part, depth + 1)))
    )
      return "OpenAPI schema composition is malformed.";
  }
  if (Object.hasOwn(schema, "not")) {
    const error = checkSchemaShape(schema.not, depth + 1);
    if (error) return error;
  }
  if (Object.hasOwn(schema, "items")) {
    const error = checkSchemaShape(schema.items, depth + 1);
    if (error) return error;
  }
  if (
    Object.hasOwn(schema, "additionalProperties") &&
    typeof schema.additionalProperties !== "boolean"
  ) {
    const error = checkSchemaShape(schema.additionalProperties, depth + 1);
    if (error) return error;
  }
  for (const child of Object.values(schema.properties ?? {})) {
    const error = checkSchemaShape(child, depth + 1);
    if (error) return error;
  }
  return null;
}

function matchesSchema(value, schema, root, depth = 0) {
  if (depth > 128) return false;
  if (typeof schema === "boolean") return schema;
  if (!isRecord(schema)) return false;
  if (Object.hasOwn(schema, "$ref")) {
    const target = resolvePointer(root, schema.$ref);
    if (target === undefined || !matchesSchema(value, target, root, depth + 1))
      return false;
  }
  if (Object.hasOwn(schema, "type") && !valueMatchesType(value, schema.type))
    return false;
  if (Object.hasOwn(schema, "const") && !isDeepStrictEqual(value, schema.const))
    return false;
  if (
    Array.isArray(schema.enum) &&
    !schema.enum.some((candidate) => isDeepStrictEqual(value, candidate))
  )
    return false;
  if (
    Array.isArray(schema.required) &&
    (!isRecord(value) ||
      schema.required.some((key) => !Object.hasOwn(value, key)))
  )
    return false;
  if (isRecord(schema.properties)) {
    if (!isRecord(value)) return false;
    for (const [key, childSchema] of Object.entries(schema.properties)) {
      if (
        Object.hasOwn(value, key) &&
        !matchesSchema(value[key], childSchema, root, depth + 1)
      )
        return false;
    }
  }
  if (isRecord(value) && Object.hasOwn(schema, "additionalProperties")) {
    const known = isRecord(schema.properties) ? schema.properties : {};
    const unknown = Object.keys(value).filter(
      (key) => !Object.hasOwn(known, key),
    );
    if (schema.additionalProperties === false && unknown.length > 0)
      return false;
    if (
      isRecord(schema.additionalProperties) ||
      typeof schema.additionalProperties === "boolean"
    ) {
      if (
        unknown.some(
          (key) =>
            !matchesSchema(
              value[key],
              schema.additionalProperties,
              root,
              depth + 1,
            ),
        )
      )
        return false;
    }
  }
  if (Array.isArray(value) && Object.hasOwn(schema, "items")) {
    if (
      value.some((item) => !matchesSchema(item, schema.items, root, depth + 1))
    )
      return false;
  }
  if (typeof value === "string") {
    if (
      (Number.isInteger(schema.minLength) &&
        [...value].length < schema.minLength) ||
      (Number.isInteger(schema.maxLength) &&
        [...value].length > schema.maxLength)
    )
      return false;
    if (typeof schema.pattern === "string") {
      try {
        if (!new RegExp(schema.pattern).test(value)) return false;
      } catch {
        return false;
      }
    }
    if (!matchesFormat(value, schema.format)) return false;
  }
  if (
    typeof value === "number" &&
    ((Number.isFinite(schema.minimum) && value < schema.minimum) ||
      (Number.isFinite(schema.maximum) && value > schema.maximum))
  )
    return false;
  if (
    Array.isArray(schema.anyOf) &&
    !schema.anyOf.some((part) => matchesSchema(value, part, root, depth + 1))
  )
    return false;
  if (
    Array.isArray(schema.oneOf) &&
    schema.oneOf.filter((part) => matchesSchema(value, part, root, depth + 1))
      .length !== 1
  )
    return false;
  if (
    Array.isArray(schema.allOf) &&
    schema.allOf.some((part) => !matchesSchema(value, part, root, depth + 1))
  )
    return false;
  if (
    Object.hasOwn(schema, "not") &&
    matchesSchema(value, schema.not, root, depth + 1)
  )
    return false;
  return true;
}

function validSchemaTypes(type) {
  return Array.isArray(type)
    ? type.length > 0 && type.every((entry) => JSON_SCHEMA_TYPES.has(entry))
    : JSON_SCHEMA_TYPES.has(type);
}

function valueMatchesType(value, type) {
  const types = Array.isArray(type) ? type : [type];
  return types.some((entry) => {
    switch (entry) {
      case "array":
        return Array.isArray(value);
      case "boolean":
        return typeof value === "boolean";
      case "integer":
        return Number.isInteger(value);
      case "null":
        return value === null;
      case "number":
        return typeof value === "number" && Number.isFinite(value);
      case "object":
        return isRecord(value);
      case "string":
        return typeof value === "string";
      default:
        return false;
    }
  });
}

function matchesFormat(value, format) {
  if (typeof format !== "string") return true;
  switch (format) {
    case "email":
      return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
    case "uri":
      try {
        return new URL(value).protocol.length > 0;
      } catch {
        return false;
      }
    case "uuid":
      return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        value,
      );
    default:
      return true;
  }
}

function checkOAuthFormParameterPolicy(specification) {
  const schemas = specification.components?.schemas;
  const schemaPolicies = [
    ["AuthorizationCodeGrant", "refresh_token"],
    ["RefreshTokenGrant", "code", "redirect_uri", "code_verifier"],
    ["IntrospectionRequest"],
    ["RevocationRequest"],
  ];
  const forbiddenCredentials = [
    "client_id",
    "client_secret",
    "client_assertion",
    "client_assertion_type",
  ];
  for (const [name, ...grantFields] of schemaPolicies) {
    const schema = schemas?.[name];
    const denied = new Set(
      Array.isArray(schema?.not?.anyOf)
        ? schema.not.anyOf
            .filter((entry) => Array.isArray(entry?.required))
            .flatMap((entry) => entry.required)
        : [],
    );
    const rejectedFields = [...forbiddenCredentials, ...grantFields];
    if (
      !isRecord(schema) ||
      schema.type !== "object" ||
      schema.additionalProperties !== true ||
      rejectedFields.some((field) => !denied.has(field)) ||
      rejectedFields.some((field) =>
        Object.hasOwn(schema.properties ?? {}, field),
      )
    )
      return "OAuth form parameter policy must allow extensions and reject body client credentials.";
  }
  for (const name of ["IntrospectionRequest", "RevocationRequest"]) {
    const hint = schemas?.[name]?.properties?.token_type_hint;
    if (
      !isRecord(hint) ||
      hint.type !== "string" ||
      ["minLength", "maxLength", "pattern", "const", "enum"].some((key) =>
        Object.hasOwn(hint, key),
      )
    )
      return "Ignored token_type_hint must not claim value constraints the HTTP parser does not enforce.";
  }
  return null;
}

export async function readBoundedApiInput(path, { lstat, readFile }) {
  let metadata;
  try {
    metadata = await lstat(path);
  } catch {
    throw new Error("Cannot inspect API reference input.");
  }
  if (!metadata.isFile())
    throw new Error("API reference input must be a regular file.");
  if (
    !Number.isSafeInteger(metadata.size) ||
    metadata.size > MAX_API_SPEC_BYTES
  )
    throw new Error("API reference input exceeds its size limit.");
  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    throw new Error("Cannot read API reference input.");
  }
  if (!(bytes instanceof Uint8Array) || bytes.byteLength > MAX_API_SPEC_BYTES)
    throw new Error("API reference input exceeds its size limit.");
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("API reference input must be valid UTF-8.");
  }
}

function checkReferences(root) {
  let error = null;
  const visit = (value) => {
    if (error) return;
    if (Array.isArray(value)) {
      for (const item of value) visit(item);
      return;
    }
    if (!isRecord(value)) return;
    if (Object.hasOwn(value, "$ref")) {
      const ref = value.$ref;
      if (typeof ref !== "string" || !ref.startsWith("#/")) {
        error = "Only local OpenAPI references are allowed.";
        return;
      }
      if (resolvePointer(root, ref) === undefined) {
        error = "OpenAPI contains an unresolved local reference.";
        return;
      }
    }
    for (const child of Object.values(value)) visit(child);
  };
  visit(root);
  return error;
}

function resolvePointer(root, pointer) {
  const segments = pointer
    .slice(2)
    .split("/")
    .map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"));
  return segments.reduce((value, segment) => {
    if (Array.isArray(value) && /^(0|[1-9]\d*)$/.test(segment))
      return value[Number(segment)];
    if (isRecord(value)) return value[segment];
    return undefined;
  }, root);
}

function resolveLocalReference(value, root) {
  const visited = new Set();
  for (let depth = 0; depth < 32; depth++) {
    if (!isRecord(value) || !Object.hasOwn(value, "$ref")) return value;
    const reference = value.$ref;
    if (
      typeof reference !== "string" ||
      !reference.startsWith("#/") ||
      visited.has(reference)
    )
      return undefined;
    visited.add(reference);
    value = resolvePointer(root, reference);
    if (value === undefined) return undefined;
  }
  return undefined;
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function validPath(value) {
  return (
    typeof value === "string" && value.startsWith("/") && value.length <= 256
  );
}
function validOperationId(value) {
  return typeof value === "string" && /^[A-Za-z][A-Za-z0-9]{0,63}$/.test(value);
}
