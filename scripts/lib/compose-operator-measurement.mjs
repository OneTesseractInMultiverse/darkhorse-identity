import { percentiles } from "./benchmark-model.mjs";

const operations = new Set([
  "account.show",
  "account.show.missing",
  "account.show.denied",
  "application.show",
  "application.show.denied",
  "client.show",
  "client.show.denied",
]);

function boundedArray(value, maximum, description) {
  if (!Array.isArray(value) || value.length > maximum)
    throw new Error(`Invalid Compose ${description} observations.`);
  return value;
}

function phase(value) {
  const summary = value?.summary;
  if (
    typeof value?.name !== "string" ||
    !Number.isSafeInteger(summary?.scheduled) ||
    !Number.isSafeInteger(summary?.attempts) ||
    !summary?.outcomes ||
    !summary?.generatorDrops ||
    !Number.isSafeInteger(summary.generatorDrops.late) ||
    !Number.isSafeInteger(summary.generatorDrops.full)
  )
    throw new Error("Invalid Compose HTTP phase summary.");
  return {
    name: value.name,
    offeredRatePerSecond: summary.offeredRate,
    scheduled: summary.scheduled,
    dispatched: summary.attempts,
    outcomes: Object.fromEntries(
      [
        "authorized",
        "denied",
        "unavailable",
        "error",
        "transport_error",
        "violation",
        "generator_late",
        "generator_full",
      ].map((key) => [
        key,
        key === "generator_late"
          ? summary.generatorDrops.late
          : key === "generator_full"
            ? summary.generatorDrops.full
            : (summary.outcomes[key] ?? 0),
      ]),
    ),
    scheduledLatencyMs: summary.allScheduledLatencyMs ?? null,
    authorizedScheduledLatencyMs: summary.authorizedScheduledLatencyMs ?? null,
    peakInFlight: summary.peakInFlight,
  };
}

function command(value) {
  if (
    ![0, 1].includes(value?.worker) ||
    !operations.has(value?.operation) ||
    !["read", "not_found", "denied"].includes(value?.result) ||
    !Number.isFinite(value?.scheduledMs) ||
    !Number.isFinite(value?.startMs) ||
    !Number.isFinite(value?.endMs) ||
    value.endMs < value.startMs
  )
    throw new Error("Invalid Compose operator command observation.");
  return {
    worker: value.worker,
    operation: value.operation,
    result: value.result,
    scheduledLatencyMs: value.endMs - value.scheduledMs,
    executionLatencyMs: value.endMs - value.startMs,
  };
}

function observation(value) {
  if (
    !Number.isSafeInteger(value?.runtimeConnections) ||
    value.runtimeConnections < 0 ||
    !Number.isSafeInteger(value?.activeRuntimeConnections) ||
    value.activeRuntimeConnections < 0 ||
    !Number.isSafeInteger(value?.waitingLocks) ||
    value.waitingLocks < 0 ||
    (value?.apiCpu !== undefined && typeof value.apiCpu !== "string") ||
    (value?.apiMemory !== undefined && typeof value.apiMemory !== "string") ||
    (value?.databaseCpu !== undefined &&
      typeof value.databaseCpu !== "string") ||
    (value?.databaseMemory !== undefined &&
      typeof value.databaseMemory !== "string")
  )
    throw new Error("Invalid Compose runtime observation.");
  return {
    runtimeConnections: value.runtimeConnections,
    activeRuntimeConnections: value.activeRuntimeConnections,
    waitingLocks: value.waitingLocks,
    ...(value.apiCpu === undefined ? {} : { apiCpu: value.apiCpu }),
    ...(value.apiMemory === undefined ? {} : { apiMemory: value.apiMemory }),
    ...(value.databaseCpu === undefined
      ? {}
      : { databaseCpu: value.databaseCpu }),
    ...(value.databaseMemory === undefined
      ? {}
      : { databaseMemory: value.databaseMemory }),
  };
}

export function composeOperatorMeasurement({
  phases,
  commands,
  observations,
  before,
  after,
}) {
  const safePhases = boundedArray(phases, 3, "HTTP phase").map(phase);
  const safeCommands = boundedArray(commands, 8, "operator command").map(
    command,
  );
  const safeObservations = boundedArray(observations, 64, "runtime").map(
    observation,
  );
  const counts = Object.fromEntries(
    ["read", "not_found", "denied"].map((result) => [
      result,
      safeCommands.filter((row) => row.result === result).length,
    ]),
  );
  const connections = safeObservations.map((row) => row.runtimeConnections);
  const activeConnections = safeObservations.map(
    (row) => row.activeRuntimeConnections,
  );
  const waitingLocks = safeObservations.map((row) => row.waitingLocks);
  const boundary = {
    ...(before === undefined ? {} : { before: observation(before) }),
    ...(after === undefined ? {} : { after: observation(after) }),
  };
  return {
    schema: 1,
    status:
      safePhases.length === 3 && safeCommands.length === 8
        ? "completed"
        : "incomplete",
    topology: "packaged-compose-api-and-percona-postgresql",
    databaseRole: "darkhorse_runtime",
    configuredPoolSizePerProcess: 5,
    httpPhases: safePhases,
    operator: {
      expectedCommands: 8,
      recordedCommands: safeCommands.length,
      outcomes: counts,
      scheduledLatencyMs: percentiles(
        safeCommands.map((row) => row.scheduledLatencyMs),
      ),
      executionLatencyMs: percentiles(
        safeCommands.map((row) => row.executionLatencyMs),
      ),
      byOperation: Object.fromEntries(
        [...new Set(safeCommands.map((row) => row.operation))].map(
          (operation) => [
            operation,
            {
              commands: safeCommands.filter(
                (row) => row.operation === operation,
              ).length,
              executionLatencyMs: percentiles(
                safeCommands
                  .filter((row) => row.operation === operation)
                  .map((row) => row.executionLatencyMs),
              ),
            },
          ],
        ),
      ),
    },
    runtime: {
      samples: safeObservations.length,
      maximumSampledConnections: connections.length
        ? Math.max(...connections)
        : 0,
      maximumSampledActiveConnections: activeConnections.length
        ? Math.max(...activeConnections)
        : 0,
      maximumSampledWaitingLocks: waitingLocks.length
        ? Math.max(...waitingLocks)
        : 0,
      samplesDuringOverlap: safeObservations,
      ...boundary,
    },
  };
}
