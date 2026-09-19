// Fixed, non-sensitive wire schema shared with the opt-in Rust profiler.
export const STAGES = [
  "total",
  "pool_acquire",
  "begin",
  "fence",
  "authenticate",
  "inspect",
  "policy_load",
  "decision",
  "commit",
];
export const UPPER_US = [
  1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000, 25000, 50000,
  100000, 250000, 500000, 1000000, 2500000, 5000000, 10000000,
];
export const MAX_FRAME = 16384;
function keys(value, expected) {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    JSON.stringify(Object.keys(value).sort()) ===
      JSON.stringify([...expected].sort())
  );
}
const counter = (value) => Number.isSafeInteger(value) && value >= 0;
function histogram(value) {
  if (
    !keys(value, ["ok", "error", "cancelled", "sum_us", "max_us", "buckets"]) ||
    ![value.ok, value.error, value.cancelled, value.sum_us, value.max_us].every(
      counter,
    ) ||
    !Array.isArray(value.buckets) ||
    value.buckets.length !== UPPER_US.length + 1 ||
    !value.buckets.every(counter)
  )
    throw new Error();
  const count = value.ok + value.error + value.cancelled;
  if (
    !counter(count) ||
    value.buckets.reduce((a, b) => a + b, 0) !== count ||
    value.sum_us < value.max_us ||
    (!count && (value.sum_us || value.max_us))
  )
    throw new Error();
  return {
    ...value,
    count,
    mean_us: count ? value.sum_us / count : null,
    percentile_upper_us: Object.fromEntries(
      [
        ["p50", 0.5],
        ["p95", 0.95],
        ["p99", 0.99],
      ].map(([name, fraction]) => [
        name,
        upper(value.buckets, count, fraction),
      ]),
    ),
  };
}
function upper(buckets, count, fraction) {
  if (!count) return null;
  const rank = Math.ceil(count * fraction);
  let accumulated = 0;
  const index = buckets.findIndex((value) => {
    accumulated += value;
    return accumulated >= rank;
  });
  return UPPER_US[index] ?? null;
}
export function parseProfile(text) {
  try {
    if (text.length > MAX_FRAME) throw new Error();
    const report = JSON.parse(text);
    if (
      !keys(report, ["schema", "upper_us", "stages"]) ||
      report.schema !== 1 ||
      JSON.stringify(report.upper_us) !== JSON.stringify(UPPER_US) ||
      !keys(report.stages, STAGES)
    )
      throw new Error();
    return {
      schema: 1,
      upper_us: [...UPPER_US],
      stages: Object.fromEntries(
        STAGES.map((name) => [name, histogram(report.stages[name])]),
      ),
    };
  } catch {
    throw new Error("Invalid benchmark profile frame.");
  }
}
