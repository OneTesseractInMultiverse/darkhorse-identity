import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import {
  validateCallback,
  validateIdToken,
} from "../../lib/reference-client.mjs";

test("callback rejects changed state/issuer and duplicate or substituted credentials", () => {
  const expected = {
    issuer: "https://issuer.example",
    state: "state",
    redirect: "https://client.example/callback?fixed=1",
  };
  const good = `${expected.redirect}&code=dc_${"ab".repeat(32)}&state=state&iss=${encodeURIComponent(expected.issuer)}`;
  assert.match(validateCallback(good, expected), /^dc_/);
  for (const value of [
    good + "&state=state",
    good + "&error=denied",
    good.replace("state=state", "state=other"),
    good.replace("fixed=1", "fixed=2"),
    good.replace("client.example", "other.example"),
    good.replace("dc_", "da_"),
    good + "#fragment",
  ]) {
    assert.throws(() => validateCallback(value, expected));
  }
});
test("ID validator refuses unsupported JWT profiles before consulting a key", () => {
  for (const header of [
    { alg: "none", typ: "JWT" },
    { alg: "RS256", typ: "logout+jwt" },
    { alg: "HS256", typ: "JWT" },
    { alg: "RS256", typ: "JWT", jku: "https://evil.example" },
  ]) {
    const jwt = `${Buffer.from(JSON.stringify(header)).toString("base64url")}.e30.AA`;
    assert.throws(() => validateIdToken(jwt, [], {}));
  }
});

test("session-aware ID validation requires a signed session reference", () => {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 3072,
  });
  const key = {
    ...publicKey.export({ format: "jwk" }),
    kid: "session-test",
    alg: "RS256",
    use: "sig",
  };
  const expected = {
    issuer: "https://issuer.example",
    client: "client",
    subject: "subject",
    nonce: "challenge",
    now: 1000,
  };
  const claims = {
    iss: expected.issuer,
    aud: expected.client,
    sub: expected.subject,
    nonce: expected.nonce,
    iat: 1000,
    exp: 1300,
    auth_time: 990,
    sid: "12345678-1234-4567-89ab-123456789abc",
  };
  const header = Buffer.from(
    JSON.stringify({ alg: "RS256", typ: "JWT", kid: key.kid }),
  ).toString("base64url");
  const encode = (value) =>
    Buffer.from(JSON.stringify(value)).toString("base64url");
  const signed = (value) => {
    const message = `${header}.${encode(value)}`;
    return `${message}.${sign("RSA-SHA256", Buffer.from(message), privateKey).toString("base64url")}`;
  };
  const original = signed(claims);
  assert.equal(validateIdToken(original, [key], expected).sid, claims.sid);
  for (const sid of [
    undefined,
    null,
    1,
    "",
    "cookie-secret",
    "00000000-0000-0000-0000-000000000000",
  ]) {
    assert.throws(() =>
      validateIdToken(signed({ ...claims, sid }), [key], expected),
    );
  }
  const changed = `${header}.${encode({ ...claims, sid: "87654321-4321-4567-89ab-123456789abc" })}.${original.split(".")[2]}`;
  assert.throws(() => validateIdToken(changed, [key], expected));
});
