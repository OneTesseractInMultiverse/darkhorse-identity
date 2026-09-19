import { percentiles, summarize } from "./benchmark-model.mjs";

export function arrivalPlan({ rate, durationMs, maxInFlight, maxLatenessMs }) {
  if (
    ![rate, durationMs, maxInFlight, maxLatenessMs].every(Number.isInteger) ||
    rate < 1 ||
    rate > 2000 ||
    durationMs < 1 ||
    durationMs > 30000 ||
    maxInFlight < 1 ||
    maxInFlight > 128 ||
    maxLatenessMs < 0 ||
    maxLatenessMs > 100 ||
    Math.ceil((rate * durationMs) / 1000) > 30000
  )
    throw new Error("Arrival workload outside safe bounds.");
  return {
    rate,
    durationMs,
    maxInFlight,
    maxLatenessMs,
    count: Math.ceil((rate * durationMs) / 1000),
    intervalMs: 1000 / rate,
  };
}
export function arrivalAction(plan, { scheduledMs, nowMs, endMs, inFlight }) {
  if (nowMs >= endMs || nowMs - scheduledMs > plan.maxLatenessMs)
    return "generator_late";
  if (inFlight >= plan.maxInFlight) return "generator_full";
  return "dispatch";
}
export function arrivalDrop(
  index,
  selection,
  outcome,
  scheduledMs,
  observedMs,
) {
  return {
    index,
    client: selection.client,
    epoch: selection.epoch,
    scheduledMs,
    observedMs,
    startMs: null,
    elapsedMs: null,
    dispatchDelayMs: null,
    scheduledLatencyMs: null,
    status: null,
    outcome,
  };
}
export function arrivalResult(row, scheduledMs) {
  return {
    ...row,
    scheduledMs,
    dispatchDelayMs: row.startMs - scheduledMs,
    scheduledLatencyMs: row.startMs - scheduledMs + row.elapsedMs,
  };
}
function totals(plan, rows, wallMs) {
  const sent = rows.filter((row) => row.startMs !== null);
  const late = rows.filter((row) => row.outcome === "generator_late").length;
  const full = rows.filter((row) => row.outcome === "generator_full").length;
  const summary = summarize(sent, wallMs);
  return {
    ...summary,
    scheduled: rows.length,
    scheduledPerSecond: rows.length / (plan.durationMs / 1000),
    scheduleDurationMs: plan.durationMs,
    generatorDrops: {
      late,
      full,
      fraction: rows.length ? (late + full) / rows.length : 0,
    },
    scheduledFailureRate: rows.length
      ? (late + full + summary.attempts * summary.errorRate) / rows.length
      : 0,
    dispatchDelayMs: percentiles(sent.map((r) => r.dispatchDelayMs)),
    allScheduledLatencyMs: percentiles(sent.map((r) => r.scheduledLatencyMs)),
    authorizedScheduledLatencyMs: percentiles(
      sent
        .filter((r) => r.outcome === "authorized")
        .map((r) => r.scheduledLatencyMs),
    ),
  };
}
export function arrivalSummary(name, plan, { rows, wallMs, peakInFlight }) {
  return {
    name,
    mode: "fixed-arrival",
    offeredRate: plan.rate,
    ...totals(plan, rows, wallMs),
    maxInFlight: plan.maxInFlight,
    maxLatenessMs: plan.maxLatenessMs,
    peakInFlight,
    byClient: Object.fromEntries(
      [...new Set(rows.map((r) => r.client))].map((client) => [
        client,
        totals(
          plan,
          rows.filter((r) => r.client === client),
          wallMs,
        ),
      ]),
    ),
  };
}
