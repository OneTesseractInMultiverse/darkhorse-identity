// Test-only issuer-bound cache; policy is independent of time and transport I/O.
import { createJwksTransport } from "./reference-jwks-https.mjs";
import {
  JWKS_LIMITS,
  trustedJwksConfiguration,
  parseJwks,
  jwksAction,
} from "./reference-jwks-policy.mjs";
export {
  JWKS_LIMITS,
  trustedJwksConfiguration,
  parseJwks,
} from "./reference-jwks-policy.mjs";

export function createJwksSource(
  configuration,
  { clock = () => performance.now(), transport = createJwksTransport() } = {},
) {
  const trusted = trustedJwksConfiguration(configuration);
  const state = {
    keys: [],
    expiresAt: 0,
    refreshAt: 0,
    pending: null,
    waiters: 0,
  };
  function refresh(started) {
    return (async () => parseJwks(await transport(trusted)))()
      .then((keys) => {
        state.keys = keys;
        state.expiresAt = started + JWKS_LIMITS.cacheMs;
      })
      .finally(() => {
        state.refreshAt = clock() + JWKS_LIMITS.cooldownMs;
        state.pending = null;
      });
  }
  return Object.freeze({
    async getKeys(issuer, kid) {
      if (
        issuer !== trusted.issuer ||
        typeof kid !== "string" ||
        !kid.length ||
        kid.length > 128
      )
        throw new Error("Untrusted JWKS key request.");
      const now = clock();
      const action = jwksAction(state, kid, now);
      if (action === "cached") return state.keys;
      if (action === "unavailable")
        throw new Error("Trusted JWKS unavailable.");
      if (action === "refresh") state.pending = refresh(now);
      state.waiters++;
      try {
        await state.pending;
        if (clock() >= state.expiresAt)
          throw new Error("Trusted JWKS expired.");
        return state.keys;
      } finally {
        state.waiters--;
      }
    },
  });
}
