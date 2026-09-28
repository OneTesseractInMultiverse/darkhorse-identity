const UPPER_US = [
  1, 5, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000,
  100_000, 250_000, 500_000, 1_000_000, 2_500_000, 5_000_000, 10_000_000,
];
const STAGES = [
  "service_slot",
  "service_handler",
  "admission_global",
  "admission_global_queue",
  "admission_global_counter",
  "admission_authenticated",
  "admission_caller_authentication",
  "admission_caller_counter",
  "limiter_slot",
  "limiter_operation",
  "total",
  "pool_acquire",
  "begin",
  "fence",
  "authenticate",
  "inspect",
  "policy_load",
  "decision",
  "commit",
  "client_total",
  "client_pool_acquire",
  "client_begin",
  "client_fence",
  "client_authenticate",
  "client_inspect",
  "client_commit",
];
const PHASES = new Set([
  "control-before",
  "account-detail-overlap",
  "control-after",
]);

export function parseComposeOperatorProfileLine(line, phase) {
  const encoded =
    typeof line === "string"
      ? line.match(/DARKHORSE_PROFILE (\{.*\})$/)?.[1]
      : undefined;
  if (!PHASES.has(phase) || !encoded)
    throw new Error("Invalid Compose database profile line.");
  try {
    return { phase, report: JSON.parse(encoded) };
  } catch {
    throw new Error("Invalid Compose database profile payload.");
  }
}

function safeCount(value) {
  return Number.isSafeInteger(value) && value >= 0;
}

function histogram(value) {
  const outcomes = [value?.ok, value?.error, value?.cancelled];
  if (
    outcomes.some((count) => !safeCount(count)) ||
    !safeCount(value?.sum_us) ||
    !safeCount(value?.max_us) ||
    !Array.isArray(value?.buckets) ||
    value.buckets.length !== UPPER_US.length + 1 ||
    value.buckets.some((count) => !safeCount(count)) ||
    outcomes.reduce((total, count) => total + count, 0) !==
      value.buckets.reduce((total, count) => total + count, 0)
  )
    throw new Error("Invalid Compose database profile histogram.");

  return {
    ok: value.ok,
    error: value.error,
    cancelled: value.cancelled,
    samples: value.ok + value.error + value.cancelled,
    sumUs: value.sum_us,
    maxUs: value.max_us,
    buckets: [...value.buckets],
  };
}

export function composeOperatorDatabaseProfile(records) {
  if (!Array.isArray(records) || records.length !== 3)
    throw new Error("Invalid Compose database profile phases.");

  const seen = new Set();
  const phases = records.map((record) => {
    if (
      !PHASES.has(record?.phase) ||
      seen.has(record.phase) ||
      record?.report?.schema !== 2 ||
      JSON.stringify(record.report.upper_us) !== JSON.stringify(UPPER_US) ||
      !record.report.stages ||
      Object.keys(record.report.stages).length !== STAGES.length ||
      STAGES.some((stage) => !(stage in record.report.stages))
    )
      throw new Error("Invalid Compose database profile report.");
    seen.add(record.phase);
    return {
      phase: record.phase,
      stages: Object.fromEntries(
        STAGES.map((stage) => [stage, histogram(record.report.stages[stage])]),
      ),
    };
  });

  if (seen.size !== PHASES.size)
    throw new Error("Incomplete Compose database profile phases.");

  return { bucketUpperUs: [...UPPER_US], phases };
}
