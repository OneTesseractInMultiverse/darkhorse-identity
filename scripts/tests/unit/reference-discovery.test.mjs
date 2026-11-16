import { test } from "node:test";
import assert from "node:assert/strict";
import {
  discoverJwks,
  trustedDiscovery,
} from "../../lib/reference-discovery.mjs";

const trusted = {
  issuer: "https://provider.example",
  url: "https://provider.example/jwks",
  ca: "explicit fixture trust",
};
const metadata = { issuer: trusted.issuer, jwks_uri: trusted.url };
test("discovery must match the independently pinned issuer and exact JWKS route", () => {
  assert.deepEqual(trustedDiscovery(metadata, trusted), trusted);
  for (const change of [
    { issuer: "https://other.example" },
    { issuer: trusted.issuer + "/" },
    { jwks_uri: "https://other.example/jwks" },
    { jwks_uri: "/jwks" },
    { jwks_uri: trusted.url + "?other" },
    { jwks_uri: trusted.url + "#key" },
    { jwks_uri: "https://user@provider.example/jwks" },
    { jwks_uri: "http://provider.example/jwks" },
    { jwks_uri: trusted.issuer + "/other" },
    { jwks_uri: undefined },
  ])
    assert.throws(() => trustedDiscovery({ ...metadata, ...change }, trusted));
});
test("discovery requests only the configured deployment and rejects ambiguous metadata", async () => {
  let calls = 0;
  const transport = async (configuration) => {
    calls++;
    assert.deepEqual(configuration, {
      ...trusted,
      url: trusted.issuer + "/.well-known/openid-configuration",
    });
    return Buffer.from(JSON.stringify(metadata));
  };
  assert.deepEqual(await discoverJwks(trusted, transport), trusted);
  assert.equal(calls, 1);
  await assert.rejects(
    discoverJwks({ ...trusted, url: "https://other.example/jwks" }, transport),
  );
  assert.equal(calls, 1);
  for (const body of [
    '{"issuer":"https://other.example","issuer":"https://provider.example","jwks_uri":"https://provider.example/jwks"}',
    "null",
    "[]",
    "{}",
    '{"issuer":',
    " ".repeat(65537),
  ])
    await assert.rejects(discoverJwks(trusted, async () => body));
  await assert.rejects(
    discoverJwks(trusted, async () => {
      throw new Error("TLS failure");
    }),
  );
});
