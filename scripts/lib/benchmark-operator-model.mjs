import { percentiles } from "./benchmark-model.mjs";

export function operatorOperations(details = false, worker = 0) {
  if (!Number.isInteger(worker) || worker < 0 || worker > 1)
    throw new Error("Unknown benchmark operator worker.");
  if (!details)
    return [
      "account.list",
      "application.list",
      "account.list",
      "application.list",
    ];
  return worker === 0
    ? [
        "account.show",
        "application.show",
        "account.show.missing",
        "client.show",
      ]
    : [
        "account.show.denied",
        "application.show.denied",
        "account.show.denied",
        "client.show.denied",
      ];
}
export function operatorRead(operation, ids) {
  const reads = {
    "account.list": {
      args: ["account", "list", "--limit", "25"],
      table: "operator_directory_audit",
      command: "account.list",
    },
    "application.list": {
      args: ["application", "list", "--limit", "25"],
      table: "operator_catalog_audit",
      command: "application.list",
    },
    "account.show": {
      args: ["account", "show", ids.principal],
      table: "operator_account_audit",
      command: "account.show",
    },
    "account.show.missing": {
      args: ["account", "show", ids.missingPrincipal],
      table: "operator_account_audit",
      command: "account.show",
      result: "not_found",
    },
    "account.show.denied": {
      args: ["account", "show", ids.principal],
      table: "operator_account_audit",
      command: "account.show",
      result: "denied",
    },
    "application.show": {
      args: ["application", "show", ids.application],
      table: "operator_catalog_detail_audit",
      command: "application.show",
    },
    "application.show.denied": {
      args: ["application", "show", ids.application],
      table: "operator_catalog_detail_audit",
      command: "application.show",
    },
    "client.show": {
      args: ["client", "show", ids.application, ids.client],
      table: "operator_catalog_detail_audit",
      command: "client.show",
    },
    "client.show.denied": {
      args: ["client", "show", ids.application, ids.client],
      table: "operator_catalog_detail_audit",
      command: "client.show",
    },
  };
  if (!Object.hasOwn(reads, operation))
    throw new Error("Unknown benchmark operator read.");
  const result = operation.endsWith(".denied") ? "denied" : "read";
  return { ...reads[operation], result: reads[operation].result ?? result };
}

export function operatorResult(output, expectedResult = "read") {
  let value;
  try {
    value = JSON.parse(output);
  } catch {
    throw new Error("Invalid benchmark operator response.");
  }
  const successful = expectedResult === "read";
  if (
    !["read", "not_found", "denied"].includes(expectedResult) ||
    value?.ok !== successful ||
    value.schema_version !== 1 ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      value.data?.operation_id ?? "",
    ) ||
    (!successful && value.error?.code !== "operation_failed")
  )
    throw new Error("Invalid benchmark operator response.");
  return { operationId: value.data.operation_id, result: expectedResult };
}
export function postCommitRevocationAccepted(phase) {
  const outcomes = phase?.outcomes;
  const attempts = phase?.attempts ?? phase?.scheduled;
  const noDrops =
    phase?.generatorDrops === undefined ||
    (phase.generatorDrops.late === 0 && phase.generatorDrops.full === 0);
  return (
    attempts === 64 &&
    noDrops &&
    ["authorized", "healthy", "error", "transport_error", "violation"].every(
      (name) => outcomes[name] === 0,
    ) &&
    outcomes.denied + outcomes.unavailable === 64
  );
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
