const profiles = Object.freeze({
  smoke: Object.freeze({
    name: "smoke",
    rate: 25,
    durationMs: 4000,
    maxInFlight: 32,
    maxLatenessMs: 25,
  }),
  baseline: Object.freeze({
    name: "baseline",
    rate: 200,
    durationMs: 8000,
    maxInFlight: 32,
    maxLatenessMs: 25,
  }),
});

export function composeOperatorProfile(name) {
  const profile = profiles[name];
  if (!profile) throw new Error("Unknown Compose operator benchmark profile.");
  return { ...profile };
}
