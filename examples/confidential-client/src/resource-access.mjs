const MAX_RESPONSE_BYTES = 16 * 1024;
const MAX_TOKEN_BYTES = 2048;
const MAX_CAPABILITIES = 256;
const RESOURCE_ID =
  /^rs_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const AUDIENCE =
  /^urn:darkhorse:resource:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

function secureIssuer(value) {
  if (typeof value !== "string" || value.length === 0) return false;
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.origin === value &&
      url.hostname.length > 0 &&
      url.username.length === 0 &&
      url.password.length === 0 &&
      url.pathname === "/" &&
      url.search.length === 0 &&
      url.hash.length === 0
    );
  } catch {
    return false;
  }
}

function validConfiguration({
  issuer,
  introspectionClientId,
  introspectionSecret,
  accessToken,
  fetch,
  timeoutMs,
}) {
  return (
    secureIssuer(issuer) &&
    typeof introspectionClientId === "string" &&
    RESOURCE_ID.test(introspectionClientId) &&
    typeof introspectionSecret === "string" &&
    /^[a-f0-9]{64}$/.test(introspectionSecret) &&
    typeof accessToken === "string" &&
    accessToken.length > 0 &&
    Buffer.byteLength(accessToken, "utf8") <= MAX_TOKEN_BYTES &&
    /^[\x21-\x7e]+$/.test(accessToken) &&
    typeof fetch === "function" &&
    Number.isInteger(timeoutMs) &&
    timeoutMs >= 100 &&
    timeoutMs <= 10_000
  );
}

async function boundedResponseText(response) {
  if (!response.body) return null;
  const contentLength = response.headers.get("content-length");
  if (contentLength && /^\d+$/.test(contentLength)) {
    if (Number(contentLength) > MAX_RESPONSE_BYTES) {
      await response.body.cancel().catch(() => {});
      return null;
    }
  }

  const reader = response.body.getReader();
  const chunks = [];
  let byteLength = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      byteLength += value.byteLength;
      if (byteLength > MAX_RESPONSE_BYTES) {
        await reader.cancel().catch(() => {});
        return null;
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(byteLength);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

function parseObject(text) {
  if (text === null) return null;
  try {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value))
      return null;
    return value;
  } catch {
    return null;
  }
}

function hasJsonContentType(response) {
  return (
    response.headers
      .get("content-type")
      ?.split(";", 1)[0]
      .trim()
      .toLowerCase() === "application/json"
  );
}

/**
 * Ask Darkhorse for a fresh decision using the protected resource's own
 * HTTP Basic credential. The helper never retries and returns no response
 * details on transport or protocol failure.
 */
export async function requestResourceIntrospection({
  issuer,
  introspectionClientId,
  introspectionSecret,
  accessToken,
  fetch = globalThis.fetch,
  timeoutMs = 3000,
} = {}) {
  const input = {
    issuer,
    introspectionClientId,
    introspectionSecret,
    accessToken,
    fetch,
    timeoutMs,
  };
  if (!validConfiguration(input)) {
    throw new TypeError("resource introspection configuration is invalid.");
  }

  const authorization = Buffer.from(
    `${encodeURIComponent(introspectionClientId)}:${encodeURIComponent(introspectionSecret)}`,
    "utf8",
  ).toString("base64");
  const body = new URLSearchParams({
    token: accessToken,
    token_type_hint: "access_token",
  });
  try {
    const response = await fetch(new URL("/introspect", issuer), {
      method: "POST",
      headers: {
        accept: "application/json",
        authorization: `Basic ${authorization}`,
        "content-type": "application/x-www-form-urlencoded",
      },
      body,
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
      signal: AbortSignal.timeout(timeoutMs),
    });
    if (response.status !== 200) {
      await response.body?.cancel().catch(() => {});
      return { status: response.status, body: null };
    }
    if (!hasJsonContentType(response)) {
      await response.body?.cancel().catch(() => {});
      return { status: 200, body: null };
    }
    return {
      status: 200,
      body: parseObject(await boundedResponseText(response)),
    };
  } catch {
    return { status: 0, body: null };
  }
}

/** Pure fail-closed authorization check for a fresh introspection response. */
export function hasResourceCapability(result, expected = {}) {
  if (!expected || typeof expected !== "object" || Array.isArray(expected)) {
    return false;
  }
  const { issuer, audience, capability, nowSeconds } = expected;
  if (
    !result ||
    result.status !== 200 ||
    !result.body ||
    typeof result.body !== "object" ||
    Array.isArray(result.body)
  ) {
    return false;
  }
  const body = result.body;
  return (
    secureIssuer(issuer) &&
    typeof audience === "string" &&
    AUDIENCE.test(audience) &&
    typeof capability === "string" &&
    UUID.test(capability) &&
    Number.isSafeInteger(nowSeconds) &&
    body.active === true &&
    body.token_type === "Bearer" &&
    body.iss === issuer &&
    body.aud === audience &&
    Number.isSafeInteger(body.iat) &&
    Number.isSafeInteger(body.exp) &&
    body.iat >= 0 &&
    body.exp > body.iat &&
    body.iat <= nowSeconds &&
    body.exp > nowSeconds &&
    body.exp - body.iat <= 300 &&
    Array.isArray(body.capabilities) &&
    body.capabilities.length <= MAX_CAPABILITIES &&
    body.capabilities.every(
      (item) => typeof item === "string" && UUID.test(item),
    ) &&
    body.capabilities.includes(capability)
  );
}
