import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import {
  validateIdToken,
  validateLogoutToken,
} from "../../lib/reference-client.mjs";

const { privateKey, publicKey } = generateKeyPairSync("rsa", {
  modulusLength: 3072,
});
const key = {
  ...publicKey.export({ format: "jwk" }),
  kid: "logout-test",
  alg: "RS256",
  use: "sig",
};
const expected = {
  issuer: "https://issuer.example",
  client: "client",
  now: 1000,
};
const event = "http://schemas.openid.net/event/backchannel-logout";
const claims = {
  iss: expected.issuer,
  aud: expected.client,
  sub: "subject",
  sid: "12345678-1234-4567-89ab-123456789abc",
  jti: "unique-token-reference",
  iat: 1000,
  exp: 1120,
  events: { [event]: {} },
};
const encode = (value) =>
  Buffer.from(JSON.stringify(value)).toString("base64url");
function signed(payload = claims, overrides = {}) {
  const header = {
    alg: "RS256",
    typ: "logout+jwt",
    kid: key.kid,
    ...overrides,
  };
  const message = `${encode(header)}.${encode(payload)}`;
  return `${message}.${sign("RSA-SHA256", Buffer.from(message), privateKey).toString("base64url")}`;
}

test("logout validation returns the exact signed session binding and ignores unknown claims", () => {
  assert.deepEqual(validateLogoutToken(signed(), [key], expected), claims);
  const extended = {
    ...claims,
    extension: "ignored",
    events: { ...claims.events, extension: {} },
  };
  assert.deepEqual(
    validateLogoutToken(signed(extended), [key], expected),
    extended,
  );
  assert.equal(
    validateLogoutToken(signed(), [key], { ...expected, now: 1119 }).sid,
    claims.sid,
  );
});

test("logout rejects wrong issuer or client, invalid times and incomplete session-specific claims", () => {
  for (const change of [
    { iss: "https://other.example" },
    { aud: "other" },
    { aud: [expected.client] },
    { sub: undefined },
    { sub: "" },
    { sub: 1 },
    { sid: undefined },
    { sid: "" },
    { sid: 1 },
    { jti: undefined },
    { jti: "" },
    { jti: 1 },
    { jti: "x".repeat(129) },
    { sid: "x".repeat(129) },
    { sub: "x".repeat(256) },
    { iat: undefined },
    { iat: -1 },
    { iat: 1001 },
    { iat: "1000" },
    { iat: 999.5 },
    { exp: undefined },
    { exp: 1000 },
    { exp: 999 },
    { exp: 1121 },
    { exp: "1120" },
    { exp: Number.MAX_SAFE_INTEGER + 1 },
    { exp: 1120.5 },
    { events: undefined },
    { events: null },
    { events: [] },
    { events: {} },
    { events: { [event]: null } },
    { events: { [event]: [] } },
    { events: { [event]: 1 } },
    { nonce: "challenge" },
    { nonce: null },
  ])
    assert.throws(() =>
      validateLogoutToken(signed({ ...claims, ...change }), [key], expected),
    );
  assert.throws(() =>
    validateLogoutToken(signed(), [key], { ...expected, now: 1120 }),
  );
  for (const now of [NaN, -1, "1000", undefined])
    assert.throws(() =>
      validateLogoutToken(signed(), [key], { ...expected, now }),
    );
  for (const field of ["iss", "aud"]) {
    const missing = { ...claims, [field]: undefined };
    const incomplete = {
      ...expected,
      [field === "iss" ? "issuer" : "client"]: undefined,
    };
    assert.throws(() =>
      validateLogoutToken(signed(missing), [key], incomplete),
    );
  }
});

test("logout validation rejects forged signatures, unknown keys and token-type substitution", () => {
  for (const header of [
    { typ: "JWT" },
    { typ: undefined },
    { alg: "none" },
    { alg: "HS256" },
    { kid: "other" },
    { kid: undefined },
    { kid: "" },
    { kid: "x".repeat(129) },
    { jku: "https://evil.example" },
    { jwk: key },
    { crit: [] },
  ])
    assert.throws(() =>
      validateLogoutToken(signed(claims, header), [key], expected),
    );
  assert.throws(() => validateLogoutToken(signed(), [], expected));
  assert.throws(() => validateLogoutToken(signed(), [key, key], expected));
  const original = signed().split(".");
  original[1] = encode({ ...claims, sid: "another-session" });
  assert.throws(() => validateLogoutToken(original.join("."), [key], expected));
  for (const token of [
    null,
    "",
    "x".repeat(8192),
    "a.b.c.d",
    "!.e30.AA",
    `${encode(null)}.e30.AA`,
    signed(null),
    signed([]),
  ])
    assert.throws(() => validateLogoutToken(token, [key], expected));
  assert.throws(() =>
    validateIdToken(signed(), [key], { ...expected, subject: claims.sub }),
  );
  const id = {
    iss: expected.issuer,
    aud: expected.client,
    sub: claims.sub,
    sid: claims.sid,
    iat: 1000,
    exp: 1120,
    auth_time: 990,
    nonce: "challenge",
  };
  assert.throws(() =>
    validateLogoutToken(signed(id, { typ: "JWT" }), [key], expected),
  );
  assert.throws(() => validateLogoutToken(signed(id), [key], expected));
});
