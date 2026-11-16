// Test-only discovery for an independently pinned Darkhorse HTTPS deployment.
import assert from "node:assert/strict";
import { createJwksTransport } from "./reference-jwks-https.mjs";
import {
  parseTrustedJson,
  trustedJwksConfiguration,
} from "./reference-jwks-policy.mjs";

function deployment(configuration) {
  const trusted = trustedJwksConfiguration(configuration);
  assert.equal(new URL(trusted.issuer).origin, trusted.issuer);
  assert.equal(trusted.url, `${trusted.issuer}/jwks`);
  return trusted;
}
export function trustedDiscovery(metadata, configuration) {
  const trusted = deployment(configuration);
  assert.ok(
    metadata !== null &&
      typeof metadata === "object" &&
      !Array.isArray(metadata),
  );
  assert.equal(metadata.issuer, trusted.issuer);
  assert.equal(metadata.jwks_uri, trusted.url);
  return trusted;
}
export async function discoverJwks(
  configuration,
  transport = createJwksTransport(),
) {
  const trusted = deployment(configuration);
  const body = await transport({
    ...trusted,
    url: `${trusted.issuer}/.well-known/openid-configuration`,
  });
  return trustedDiscovery(parseTrustedJson(body), trusted);
}
