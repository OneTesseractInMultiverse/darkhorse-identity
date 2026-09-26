export const MAX_API_SPEC_BYTES = 2 * 1024 * 1024;
const METHODS = new Set(["get", "post", "put", "patch", "delete"]);

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
        !isRecord(operation.responses)
      )
        return fail(
          "Every operation needs a unique ID and at least one response.",
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
  const parameterError = checkOAuthFormParameterPolicy(specification);
  return parameterError
    ? fail(parameterError)
    : { ok: true, operationCount: documented.size };
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
