// Test-only confidential client. All deployed server behavior remains in Rust.
import { request } from "node:https";
import { createPublicKey, verify } from "node:crypto";

function requireValid(condition) {
  if (!condition) throw new Error("Reference client rejected protocol input.");
}
export function validateIdToken(token, keys, expected) {
  requireValid(typeof token === "string" && token.length < 8192);
  const parts = token.split(".");
  requireValid(
    parts.length === 3 && parts.every((p) => /^[A-Za-z0-9_-]+$/.test(p)),
  );
  const [header, claims] = parts
    .slice(0, 2)
    .map((p) => JSON.parse(Buffer.from(p, "base64url")));
  requireValid(
    header.alg === "RS256" &&
      header.typ === "JWT" &&
      !header.crit &&
      !header.jku &&
      !header.jwk,
  );
  const matching = keys.filter(
    (k) =>
      k.kid === header.kid &&
      k.kty === "RSA" &&
      k.alg === "RS256" &&
      k.use === "sig",
  );
  requireValid(matching.length === 1);
  requireValid(
    verify(
      "RSA-SHA256",
      Buffer.from(parts.slice(0, 2).join(".")),
      createPublicKey({ key: matching[0], format: "jwk" }),
      Buffer.from(parts[2], "base64url"),
    ),
  );
  requireValid(
    claims.iss === expected.issuer &&
      claims.aud === expected.client &&
      claims.sub === expected.subject &&
      claims.nonce === expected.nonce,
  );
  requireValid(
    [claims.iat, claims.exp, claims.auth_time].every(Number.isSafeInteger),
  );
  requireValid(
    claims.iat <= expected.now &&
      claims.exp > expected.now &&
      claims.exp > claims.iat &&
      claims.exp - claims.iat <= 300 &&
      claims.auth_time <= claims.iat,
  );
  requireValid(!Object.hasOwn(claims, "events"));
  return claims;
}
export function validateCallback(returned, expected) {
  const url = new URL(returned);
  const original = new URL(expected.redirect);
  requireValid(
    url.origin === original.origin &&
      url.pathname === original.pathname &&
      url.hash === "",
  );
  for (const key of ["code", "state", "iss"])
    requireValid(url.searchParams.getAll(key).length === 1);
  for (const [key, value] of original.searchParams)
    requireValid(url.searchParams.get(key) === value);
  requireValid(
    url.searchParams.get("state") === expected.state &&
      url.searchParams.get("iss") === expected.issuer &&
      !url.searchParams.has("error"),
  );
  const code = url.searchParams.get("code");
  requireValid(/^dc_[0-9a-f]{64}$/.test(code));
  return code;
}
export function exchange(
  origin,
  ca,
  client,
  secret,
  redirect,
  code,
  verifier,
  extraHeaders = {},
  discardResponse = false,
) {
  const form = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: redirect,
    code_verifier: verifier,
  }).toString();
  const basic = Buffer.from(
    `${encodeURIComponent(client)}:${encodeURIComponent(secret)}`,
  ).toString("base64");
  return send(
    origin,
    ca,
    "/token",
    {
      method: "POST",
      headers: {
        authorization: `Basic ${basic}`,
        "content-type": "application/x-www-form-urlencoded",
        ...extraHeaders,
      },
    },
    form,
    discardResponse,
  );
}
export function userinfo(origin, ca, access) {
  return send(origin, ca, "/userinfo", {
    headers: { authorization: `Bearer ${access}` },
  });
}
function send(origin, ca, path, options, body, discardResponse = false) {
  return new Promise((resolve, reject) => {
    const req = request(
      `${origin}${path}`,
      { ca, timeout: 5000, ...options },
      (res) => {
        if (discardResponse) {
          // Disconnect after committed response headers, before reading credentials.
          resolve({ status: res.statusCode });
          res.destroy();
          return;
        }
        let data = "";
        res.on("data", (chunk) => {
          data += chunk;
          if (data.length > 16384)
            req.destroy(new Error("Reference response too large."));
        });
        res.on("error", () =>
          reject(new Error("Reference response interrupted.")),
        );
        res.on("end", () => {
          try {
            resolve({
              status: res.statusCode,
              headers: res.headers,
              body: JSON.parse(data),
            });
          } catch {
            reject(new Error("Invalid reference response."));
          }
        });
      },
    );
    req.on("error", () => reject(new Error("Reference request failed.")));
    req.on("timeout", () => req.destroy());
    req.end(body);
  });
}
