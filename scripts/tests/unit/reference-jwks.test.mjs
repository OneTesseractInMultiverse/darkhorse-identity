import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import {
  JWKS_LIMITS,
  createJwksSource,
  parseJwks,
  trustedJwksConfiguration,
} from "../../lib/reference-jwks.mjs";

const configuration = {
  issuer: "https://issuer.example",
  url: "https://keys.example/trusted/jwks",
};
const { publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
const key = {
  ...publicKey.export({ format: "jwk" }),
  kid: "old",
  alg: "RS256",
  use: "sig",
};
const rotated = { ...key, kid: "new" };
const document = (keys = [key]) => JSON.stringify({ keys });
function fixture(transport) {
  let now = 0,
    calls = 0;
  const source = createJwksSource(configuration, {
    clock: () => now,
    transport: async (actual) => {
      assert.equal(actual.issuer, configuration.issuer);
      assert.equal(actual.url, configuration.url);
      calls++;
      return transport();
    },
  });
  return {
    get: (kid = key.kid, issuer = configuration.issuer) =>
      source.getKeys(issuer, kid),
    time: (value) => {
      now = value;
    },
    calls: () => calls,
  };
}

test("trust configuration binds an exact issuer to an explicit HTTPS URL", () => {
  assert.equal(trustedJwksConfiguration(configuration).url, configuration.url);
  for (const change of [
    { issuer: "" },
    { issuer: "http://issuer.example" },
    { issuer: "https://user@issuer.example" },
    { issuer: "https://issuer.example?other" },
    { url: undefined },
    { url: "http://keys.example" },
    { url: "https://user:password@keys.example" },
    { url: "https://keys.example/#fragment" },
    { url: "/jwks" },
    { url: "https://keys.example/" + "x".repeat(2048) },
    { ca: "" },
    { ca: "x".repeat(32769) },
    { ca: 1 },
  ])
    assert.throws(() =>
      trustedJwksConfiguration({ ...configuration, ...change }),
    );
});

test("a snapshot that expires during retrieval cannot be returned to a waiter", async () => {
  let finish;
  const f = fixture(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const pending = f.get();
  f.time(JWKS_LIMITS.cacheMs);
  finish(document());
  await assert.rejects(pending);
  await assert.rejects(f.get());
  assert.equal(f.calls(), 1);
});

test("JWKS accepts only bounded, unambiguous, usable public RS256 verification keys", () => {
  assert.deepEqual(parseJwks(document()), [key]);
  const immutable = parseJwks(Buffer.from(document()));
  assert.throws(() => immutable.push(rotated));
  assert.throws(() => {
    immutable[0].kid = "changed";
  });
  assert.deepEqual(parseJwks(document([{ ...key, key_ops: ["verify"] }])), [
    key,
  ]);
  for (const change of [
    { kid: "" },
    { kid: 1 },
    { kid: "x".repeat(129) },
    { kty: "EC" },
    { alg: "HS256" },
    { use: "enc" },
    { n: "AA" },
    { n: key.n + "=" },
    { n: "A" + key.n },
    { n: "_".repeat(684) },
    { n: key.n.slice(0, -1) + "A" },
    { e: "AA" },
    { e: "Ag" },
    { d: "private" },
    { key_ops: ["sign"] },
    { key_ops: ["verify", "verify"] },
    { x5u: "https://attacker.example/key" },
    { x5c: [] },
  ])
    assert.throws(() => parseJwks(document([{ ...key, ...change }])));
  for (const value of [
    "invalid",
    undefined,
    Buffer.from([0xc3, 0x28]),
    '"' + '\\"'.repeat(16000),
    "null",
    "[]",
    "{}",
    document([]),
    document([null]),
    document([key, key]),
    document([key, { ...rotated, alg: "none" }]),
    document(
      Array.from({ length: JWKS_LIMITS.keys + 1 }, (_, i) => ({
        ...key,
        kid: String(i),
      })),
    ),
    document().replace('"kid":"old"', '"kid":"other","kid":"old"'),
    '{"keys":[],"keys":' + JSON.stringify([key]) + "}",
    document().replace('"kid":"old"', '"k\\u0069d":"other","kid":"old"'),
    document() + " ".repeat(JWKS_LIMITS.responseBytes),
    '{"nested":'.repeat(10) + "0" + "}".repeat(10),
  ])
    assert.throws(() => parseJwks(value));
});

test("cold retrieval, hits, unknown-kid rotation and cache expiry use only the configured authority", async () => {
  let keys = [key];
  const f = fixture(() => document(keys));
  await assert.rejects(f.get("old", "https://attacker.example"));
  await assert.rejects(f.get("x".repeat(129)));
  assert.equal(f.calls(), 0);
  assert.deepEqual(await f.get(), [key]);
  assert.deepEqual(await f.get(), [key]);
  assert.equal(f.calls(), 1);
  keys = [key, rotated];
  await assert.rejects(f.get("new"));
  assert.equal(f.calls(), 1);
  f.time(JWKS_LIMITS.cooldownMs);
  assert.deepEqual(await f.get("new"), keys);
  assert.deepEqual(await f.get(), keys);
  assert.equal(f.calls(), 2);
  keys = [rotated];
  f.time(JWKS_LIMITS.cooldownMs + JWKS_LIMITS.cacheMs);
  assert.deepEqual(await f.get("new"), [rotated]);
  await assert.rejects(f.get());
  assert.equal(f.calls(), 3);
});

test("single-flight bounds waiters and random unknown IDs share one cooldown", async () => {
  let finish;
  const f = fixture(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const pending = Array.from({ length: JWKS_LIMITS.waiters }, (_, i) =>
    f.get(String(i)),
  );
  await assert.rejects(f.get("over-capacity"));
  assert.equal(f.calls(), 1);
  finish(document());
  for (const keys of await Promise.all(pending)) assert.deepEqual(keys, [key]);
  for (let i = 0; i < 100; i++) await assert.rejects(f.get("random-" + i));
  assert.equal(f.calls(), 1);
  assert.deepEqual(await f.get(), [key]);
});

test("outage and malformed refresh preserve a fresh snapshot but never extend its lifetime", async () => {
  let result = document();
  const f = fixture(() => {
    if (result instanceof Error) throw result;
    return result;
  });
  await f.get();
  result = new Error("outage");
  f.time(JWKS_LIMITS.cooldownMs);
  await assert.rejects(f.get("new"));
  assert.deepEqual(await f.get(), [key]);
  result = document([key, key]);
  f.time(JWKS_LIMITS.cooldownMs * 2);
  await assert.rejects(f.get("new"));
  assert.deepEqual(await f.get(), [key]);
  f.time(JWKS_LIMITS.cacheMs);
  await assert.rejects(f.get());
  result = document([rotated]);
  await assert.rejects(f.get("new"));
  f.time(JWKS_LIMITS.cacheMs + JWKS_LIMITS.cooldownMs);
  assert.deepEqual(await f.get("new"), [rotated]);
  assert.equal(f.calls(), 5);
});

test("cooldown starts at completion and cache lifetime starts before the fetch", async () => {
  let finish;
  const f = fixture(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const pending = f.get();
  f.time(1900);
  finish(document());
  await pending;
  f.time(JWKS_LIMITS.cooldownMs);
  await assert.rejects(f.get("new"));
  assert.equal(f.calls(), 1);
  f.time(JWKS_LIMITS.cacheMs);
  const expired = f.get();
  assert.equal(f.calls(), 2);
  finish(document([rotated]));
  assert.deepEqual(await expired, [rotated]);
});
