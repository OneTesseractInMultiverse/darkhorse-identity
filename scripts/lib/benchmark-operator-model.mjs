import { percentiles } from "./benchmark-model.mjs";

export function operatorOperations(details = false) {
  return details
    ? ["account.show", "application.show", "account.show", "client.show"]
    : ["account.list", "application.list", "account.list", "application.list"];
}
export function operatorRead(operation, ids) {
  const reads = {
    "account.list": {
      args: ["account", "list", "--limit", "25"],
      table: "operator_directory_audit",
    },
    "application.list": {
      args: ["application", "list", "--limit", "25"],
      table: "operator_catalog_audit",
    },
    "account.show": {
      args: ["account", "show", ids.principal],
      table: "operator_account_audit",
    },
    "application.show": {
      args: ["application", "show", ids.application],
      table: "operator_catalog_detail_audit",
    },
    "client.show": {
      args: ["client", "show", ids.application, ids.client],
      table: "operator_catalog_detail_audit",
    },
  };
  if (!Object.hasOwn(reads, operation))
    throw new Error("Unknown benchmark operator read.");
  return { ...reads[operation], result: "read" };
}

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
    commandScheduledLatencyMs: percentiles(
      commands.map((row) => row.endMs - (row.scheduledMs ?? row.startMs)),
    ),
    byOperation: Object.fromEntries(
      [...new Set(commands.map((row) => row.operation))].map((operation) => [
        operation,
        {
          commands: commands.filter((row) => row.operation === operation)
            .length,
          scheduledLatencyMs: percentiles(
            commands
              .filter((row) => row.operation === operation)
              .map((row) => row.endMs - (row.scheduledMs ?? row.startMs)),
          ),
        },
      ]),
    ),
    requestsDuringCommands: during.length,
  };
}

export function operatorLimits(poolSize, restrictedDatabase = false) {
  return {
    readWorkers: 2,
    callsPerWorker: 4,
    commandDeadlineMs: 30000,
    combinedOutputBytes: 65536,
    databaseConnectionsPerCli: Math.min(poolSize, 2),
    limiterConnectionsPerCli: 1,
    configuredReadPhaseDatabaseEnvelope: poolSize + 2 * Math.min(poolSize, 2),
    databaseRole: restrictedDatabase ? "darkhorse_runtime" : "postgres",
    note: restrictedDatabase
      ? "Configured maxima, not observed peaks. CLI and HTTP use deployment runtime grants; container and replica qualification remain separate."
      : "Configured maxima, not observed peaks. CLI and HTTP use the disposable fixture database owner; production restricted-role/container qualification is separate.",
  };
}
