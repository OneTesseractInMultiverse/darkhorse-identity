// Pure workload definitions and redacted result computations.
export function benchmarkProfile(name) {
  if (name === "smoke")
    return { name, requests: 128, clients: 4, concurrency: [1, 8, 32] };
  if (name === "baseline")
    return { name, requests: 2048, clients: 8, concurrency: [1, 8, 32, 64] };
  if (["profile-smoke", "profile-baseline"].includes(name))
    return {
      ...benchmarkProfile(name.replace("profile-", "arrival-")),
      name,
      profiling: true,
    };
  if (["arrival-smoke", "arrival-baseline"].includes(name))
    return {
      name,
      requests: 128,
      clients: name === "arrival-smoke" ? 4 : 8,
      arrivals: {
        durationMs: name === "arrival-smoke" ? 2000 : 10000,
        rates: name === "arrival-smoke" ? [200, 1200] : [200, 800, 1600],
        noiseRate: 1200,
        changeRate: 200,
        maxInFlight: 128,
        maxLatenessMs: 5,
      },
    };
  throw new Error(
    "Unknown benchmark profile; use smoke, baseline, arrival-smoke, arrival-baseline, profile-smoke or profile-baseline.",
  );
}
export function classify(response, expected) {
  if (response.status === 0) return "transport_error";
  if ([429, 503].includes(response.status)) return "unavailable";
  if (expected.status === 401 && response.status === 401) return "denied";
  if (response.status !== 200) return "error";
  if (expected.status === 200) return "healthy";
  if (expected.status === 401) return "violation";
  if (
    expected.active === false ||
    (expected.allowInactive && response.body?.active === false)
  )
    return JSON.stringify(response.body) === '{"active":false}'
      ? "denied"
      : "violation";
  if (!response.body || response.body.active !== true) return "violation";
  for (const key of ["iss", "aud", "sub", "client_id", "scope"])
    if (response.body[key] !== expected[key]) return "violation";
  const actual = response.body.capabilities;
  if (!Array.isArray(actual) || actual.some((c) => typeof c !== "string"))
    return "violation";
  const sorted = [...actual].sort();
  const allowed = [expected.capabilities, ...(expected.alternatives ?? [])];
  return allowed.some(
    (caps) => JSON.stringify([...caps].sort()) === JSON.stringify(sorted),
  )
    ? "authorized"
    : "violation";
}
export function percentiles(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const at = (p) => sorted[Math.ceil(p * sorted.length) - 1];
  return { p50: at(0.5), p95: at(0.95), p99: at(0.99) };
}
export function summarize(rows, wallMs) {
  const outcomes = Object.fromEntries(
    [
      "authorized",
      "healthy",
      "denied",
      "unavailable",
      "error",
      "transport_error",
      "violation",
    ].map((o) => [o, rows.filter((r) => r.outcome === o).length]),
  );
  return {
    attempts: rows.length,
    wallMs,
    outcomes,
    attemptsPerSecond: rows.length / (wallMs / 1000),
    authorizedPerSecond: outcomes.authorized / (wallMs / 1000),
    denialRate: rows.length ? outcomes.denied / rows.length : 0,
    errorRate: rows.length
      ? (outcomes.unavailable +
          outcomes.error +
          outcomes.transport_error +
          outcomes.violation) /
        rows.length
      : 0,
    allLatencyMs: percentiles(rows.map((r) => r.elapsedMs)),
    authorizedLatencyMs: percentiles(
      rows.filter((r) => r.outcome === "authorized").map((r) => r.elapsedMs),
    ),
  };
}
export function validateLoad(count, concurrency) {
  if (
    !Number.isInteger(count) ||
    count < 1 ||
    count > 8192 ||
    !Number.isInteger(concurrency) ||
    concurrency < 1 ||
    concurrency > 128
  )
    throw new Error("Benchmark load outside safe bounds.");
}

export function phaseSummary(name, concurrency, rows, wallMs, change) {
  return {
    name,
    mode: "closed-loop",
    concurrency,
    ...summarize(rows, wallMs),
    change,
    byClient: Object.fromEntries(
      [...new Set(rows.map((r) => r.client))].map((client) => [
        client,
        summarize(
          rows.filter((r) => r.client === client),
          wallMs,
        ),
      ]),
    ),
  };
}
