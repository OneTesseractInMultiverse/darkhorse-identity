import { percentiles } from "./benchmark-model.mjs";

export function operatorResult(stdout) {
  let value;
  try {
    value = JSON.parse(stdout);
  } catch {
    throw new Error("Invalid benchmark operator response.");
  }
  if (
    value?.ok !== true ||
    value.schema_version !== 1 ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      value.data?.operation_id ?? "",
    )
  )
    throw new Error("Invalid benchmark operator response.");
  return { operationId: value.data.operation_id };
}
export function operatorSummary(commands, rows) {
  const during = rows.filter(
    (row) =>
      row.startMs !== null &&
      commands.some(
        (command) =>
          row.startMs >= command.startMs && row.startMs < command.endMs,
      ),
  );
  return {
    allLatencyDuringCommandsMs: percentiles(during.map((row) => row.elapsedMs)),
    authorizedScheduledLatencyDuringCommandsMs: percentiles(
      during
        .filter((row) => row.outcome === "authorized")
        .map((row) => row.scheduledLatencyMs),
    ),
    commands: commands.map((command) => ({
      ...command,
      requestsDuring: rows.filter(
        (row) =>
          row.startMs !== null &&
          row.startMs >= command.startMs &&
          row.startMs < command.endMs,
      ).length,
    })),
    commandLatencyMs: percentiles(
      commands.map((row) => row.endMs - row.startMs),
    ),
    requestsDuringCommands: during.length,
  };
}

export function operatorLimits(poolSize) {
  return {
    readWorkers: 2,
    callsPerWorker: 4,
    commandDeadlineMs: 30000,
    combinedOutputBytes: 65536,
    databaseConnectionsPerCli: Math.min(poolSize, 2),
    limiterConnectionsPerCli: 1,
    configuredReadPhaseDatabaseEnvelope: poolSize + 2 * Math.min(poolSize, 2),
    note: "Configured maxima, not observed peaks. CLI and HTTP use the disposable fixture database owner; production restricted-role/container qualification is separate.",
  };
}
