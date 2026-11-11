// Test-only, deliberately narrow Darkhorse public-key profile.
import { createPublicKey } from "node:crypto";

export const JWKS_LIMITS = Object.freeze({
  timeoutMs: 2000,
  responseBytes: 65536,
  headerBytes: 8192,
  headers: 32,
  keys: 8,
  cooldownMs: 5000,
  cacheMs: 60000,
  waiters: 32,
});
function requireValid(condition) {
  if (!condition) throw new Error("Invalid trusted JWKS input.");
}
function object(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
function httpsUrl(value) {
  requireValid(typeof value === "string" && value.length <= 2048);
  const url = new URL(value);
  requireValid(
    url.protocol === "https:" && !url.username && !url.password && !url.hash,
  );
  return url;
}
export function trustedJwksConfiguration({ issuer, url, ca }) {
  const authority = httpsUrl(issuer);
  requireValid(!authority.search);
  const endpoint = httpsUrl(url);
  requireValid(
    ca === undefined ||
      (typeof ca === "string" &&
        ca.length > 0 &&
        Buffer.byteLength(ca) <= 32768),
  );
  return Object.freeze({ issuer, url: endpoint.href, ca });
}
// JSON.parse alone silently accepts duplicate members. Scan only strings and
// structural delimiters, then let JSON.parse validate the complete grammar.
function uniqueMembers(text) {
  const stack = [];
  for (let index = 0; index < text.length; index++) {
    const token = text[index];
    const frame = stack.at(-1);
    if (token === '"') {
      const start = index++;
      while (index < text.length && text[index] !== '"')
        index += text[index] === "\\" ? 2 : 1;
      requireValid(index < text.length);
      if (frame?.key) {
        const name = JSON.parse(text.slice(start, index + 1));
        requireValid(!frame.names.has(name));
        frame.names.add(name);
        frame.key = false;
      }
    } else if (token === "{" || token === "[") {
      requireValid(stack.length < 8);
      stack.push(token === "{" ? { names: new Set(), key: true } : null);
    } else if (token === "}" || token === "]") stack.pop();
    else if (token === "," && frame) frame.key = true;
  }
}
function publicKey(value) {
  requireValid(object(value));
  requireValid(
    Object.keys(value).every((name) =>
      ["kid", "kty", "alg", "use", "n", "e", "key_ops"].includes(name),
    ),
  );
  requireValid(
    typeof value.kid === "string" &&
      value.kid.length > 0 &&
      value.kid.length <= 128,
  );
  requireValid(
    value.kty === "RSA" && value.alg === "RS256" && value.use === "sig",
  );
  requireValid(
    !Object.hasOwn(value, "key_ops") ||
      (Array.isArray(value.key_ops) &&
        value.key_ops.length === 1 &&
        value.key_ops[0] === "verify"),
  );
  requireValid(
    typeof value.n === "string" &&
      /^[A-Za-z0-9_-]{342,683}$/.test(value.n) &&
      value.e === "AQAB",
  );
  const modulus = Buffer.from(value.n, "base64url");
  requireValid(
    modulus.toString("base64url") === value.n &&
      modulus[0] !== 0 &&
      (modulus.at(-1) & 1) === 1,
  );
  const key = {
    kty: "RSA",
    n: value.n,
    e: value.e,
    kid: value.kid,
    alg: "RS256",
    use: "sig",
  };
  const imported = createPublicKey({ key, format: "jwk" });
  const bits = imported.asymmetricKeyDetails.modulusLength;
  requireValid(
    imported.asymmetricKeyType === "rsa" && bits >= 2048 && bits <= 4096,
  );
  return Object.freeze(key);
}
export function parseJwks(body) {
  requireValid(
    (typeof body === "string" || Buffer.isBuffer(body)) &&
      Buffer.byteLength(body) <= JWKS_LIMITS.responseBytes,
  );
  const text =
    typeof body === "string"
      ? body
      : new TextDecoder("utf-8", { fatal: true }).decode(body);
  uniqueMembers(text);
  const value = JSON.parse(text);
  requireValid(
    object(value) &&
      Object.keys(value).length === 1 &&
      Array.isArray(value.keys) &&
      value.keys.length > 0 &&
      value.keys.length <= JWKS_LIMITS.keys,
  );
  const keys = value.keys.map(publicKey);
  requireValid(new Set(keys.map((key) => key.kid)).size === keys.length);
  return Object.freeze(keys);
}
export function jwksAction(state, kid, now) {
  if (now < state.expiresAt && state.keys.some((key) => key.kid === kid))
    return "cached";
  if (state.waiters >= JWKS_LIMITS.waiters) return "unavailable";
  if (state.pending) return "join";
  return now < state.refreshAt ? "unavailable" : "refresh";
}
export function validJwksResponse(status, headers, count) {
  const length = headers["content-length"];
  return (
    status === 200 &&
    count <= JWKS_LIMITS.headers &&
    /^(application\/json|application\/jwk-set\+json)(?:;\s*charset=utf-8)?$/i.test(
      headers["content-type"] ?? "",
    ) &&
    (headers["content-encoding"] === undefined ||
      headers["content-encoding"] === "identity") &&
    (length === undefined ||
      (/^\d+$/.test(length) && Number(length) <= JWKS_LIMITS.responseBytes))
  );
}
