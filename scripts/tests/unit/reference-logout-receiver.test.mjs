import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import {
  parseLogoutRequest,
  receiveLogout,
} from "../../lib/reference-logout-receiver.mjs";

const { privateKey, publicKey } = generateKeyPairSync("rsa", {
  modulusLength: 3072,
});
const key = {
  ...publicKey.export({ format: "jwk" }),
  kid: "receiver",
  alg: "RS256",
  use: "sig",
};
const context = {
  issuer: "https://issuer.example",
  client: "client",
  now: 1000,
  keys: [key],
};
const claims = {
  iss: context.issuer,
  aud: context.client,
  sub: "subject",
  sid: "session",
  jti: "token",
  iat: 1000,
  exp: 1120,
  events: { "http://schemas.openid.net/event/backchannel-logout": {} },
};
function request(overrides = {}, header = {}) {
  const encode = (v) => Buffer.from(JSON.stringify(v)).toString("base64url");
  const message = `${encode({ alg: "RS256", typ: "logout+jwt", kid: key.kid, ...header })}.${encode({ ...claims, ...overrides })}`;
  const token = `${message}.${sign("RSA-SHA256", Buffer.from(message), privateKey).toString("base64url")}`;
  return {
    method: "POST",
    type: "application/x-www-form-urlencoded",
    body: new URLSearchParams({ logout_token: token }).toString(),
  };
}

test("receiver accepts exactly one bounded form token and rejects malformed transport", () => {
  assert.equal(typeof parseLogoutRequest(request()), "string");
  for (const changed of [
    { method: "GET" },
    { type: "application/json" },
    { body: "" },
    { body: "logout_token=a&logout_token=b" },
    { body: "logout_token=" },
    { body: "logout_token=a&unexpected=b" },
    { body: "logout_token=" + "x".repeat(16384) },
  ])
    assert.throws(() => parseLogoutRequest({ ...request(), ...changed }));
});

test("invalid signatures, claims and purpose never reach session persistence", () => {
  const store = {
    accept: () => assert.fail("invalid input reached persistence"),
  };
  for (const input of [
    request({ iss: "https://other.example" }),
    request({ aud: "other" }),
    request({ exp: 1000 }),
    request({ nonce: null }),
    request({}, { typ: "JWT" }),
    request({}, { kid: "unknown" }),
    { ...request(), body: "logout_token=forged" },
  ])
    assert.equal(receiveLogout(input, context, store), 400);
});

test("success is acknowledged only after atomic persistence; failures remain retryable", () => {
  let called = false;
  assert.equal(
    receiveLogout(request(), context, {
      accept: (value, fingerprint) => {
        assert.deepEqual(value, claims);
        assert.match(fingerprint, /^[0-9a-f]{64}$/);
        called = true;
        return "accepted";
      },
    }),
    204,
  );
  assert.equal(called, true);
  assert.equal(
    receiveLogout(request(), context, { accept: () => "duplicate" }),
    204,
  );
  assert.equal(
    receiveLogout(request(), context, { accept: () => "invalid" }),
    400,
  );
  assert.equal(
    receiveLogout(request(), context, {
      accept: () => {
        throw new Error("private storage detail");
      },
    }),
    503,
  );
});
