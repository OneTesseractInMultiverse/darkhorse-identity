import { benchmarkProfile } from "./benchmark-model.mjs";

// Explicit inputs keep benchmark tuning independent of deployment configuration.
export function benchmarkSettings(env) {
  const value = env.BENCH_POOL_SIZE === undefined ? "5" : env.BENCH_POOL_SIZE;
  const cacheValue =
    env.BENCH_AUTHORIZATION_CACHE_ENABLED === undefined
      ? "false"
      : env.BENCH_AUTHORIZATION_CACHE_ENABLED;
  if (
    typeof value !== "string" ||
    !/^[1-9][0-9]?$/.test(value) ||
    value.trim() !== value ||
    Number(value) > 32
  )
    throw new Error("BENCH_POOL_SIZE must be an integer from 1 through 32.");
  if (cacheValue !== "true" && cacheValue !== "false")
    throw new Error("BENCH_AUTHORIZATION_CACHE_ENABLED must be true or false.");
  return {
    poolSize: Number(value),
    authorizationCacheEnabled: cacheValue === "true",
    profile: benchmarkProfile(env.BENCH_PROFILE ?? "smoke"),
  };
}
