import {
  UsageError,
  protectedStdin,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
const invalid = () =>
  new UsageError(
    "Select a supported catalog operation with its required identifiers and selectors. Application writes require a complete specification. Client updates require scoped identifiers, revision and confirmation, with configuration in protected stdin. See docs/operator-catalog.md.",
  );
export function catalogOptions(values, stdinIsTTY) {
  protectedStdin(stdinIsTTY);
  const target = values.CATALOG_TARGET,
    operation = values.CATALOG_OPERATION || "list";
  if (
    !["application", "client"].includes(target) ||
    !["list", "show", "create", "update"].includes(operation) ||
    [
      "ACCOUNT_OPERATION",
      "ACCOUNT_ID",
      "ACCOUNT_REVISION",
      "ACCOUNT_CONFIRM",
      "ACCOUNT_SEARCH",
      "ACCOUNT_STATUS",
      "ACCOUNT_AFTER",
      "ACCOUNT_LIMIT",
    ].some((key) => Boolean(values[key]))
  )
    throw invalid();
  if (target === "client" && ["create", "update"].includes(operation))
    return clientUpdateOptions(values, operation);
  if (["create", "update"].includes(operation))
    return mutationOptions(values, target, operation);
  if (
    [
      "CATALOG_CONFIRM",
      "CATALOG_REVISION",
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
    ].some((key) => Boolean(values[key]))
  )
    throw invalid();
  const selectors =
    operation === "show"
      ? detailOptions(values, target)
      : pageOptions(values, target);
  return [
    "--auth-stdin",
    "--output",
    "json",
    "operator",
    target,
    operation,
    ...selectors,
  ];
}
function clientUpdateOptions(values, operation) {
  const application = values.CATALOG_APPLICATION_ID,
    client = values.CATALOG_CLIENT_ID,
    revision = values.CATALOG_REVISION ?? "";
  if (
    operation !== "update" ||
    !nonzeroUuid(application) ||
    !nonzeroUuid(client) ||
    values.CATALOG_CONFIRM !== "yes" ||
    !/^(0|[1-9][0-9]{0,18})$/.test(revision) ||
    BigInt(revision) > 9223372036854775807n ||
    [
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
      "CATALOG_STATUS",
      "CATALOG_SEARCH",
      "CATALOG_AFTER",
      "CATALOG_LIMIT",
    ].some((key) => Boolean(values[key]))
  )
    throw invalid();
  return [
    "--auth-stdin",
    "--output",
    "json",
    "--yes",
    "operator",
    "client",
    "update",
    application,
    client,
    revision,
  ];
}
function mutationOptions(values, target, operation) {
  const name = values.CATALOG_NAME ?? "",
    owner = values.CATALOG_OWNER_ID,
    application = values.CATALOG_APPLICATION_ID ?? "",
    revision = values.CATALOG_REVISION ?? "";
  if (
    target !== "application" ||
    values.CATALOG_CONFIRM !== "yes" ||
    !name.trim() ||
    Array.from(name.trim()).length > 100 ||
    /[\p{Cc}]/u.test(name.trim()) ||
    Buffer.byteLength(name) > 1024 ||
    !nonzeroUuid(owner) ||
    !["active", "inactive"].includes(values.CATALOG_STATUS) ||
    [
      "CATALOG_CLIENT_ID",
      "CATALOG_SEARCH",
      "CATALOG_AFTER",
      "CATALOG_LIMIT",
    ].some((key) => Boolean(values[key])) ||
    (operation === "create"
      ? application !== "" || revision !== ""
      : !nonzeroUuid(application) ||
        !/^(0|[1-9][0-9]{0,18})$/.test(revision) ||
        BigInt(revision) > 9223372036854775807n)
  )
    throw invalid();
  return [
    "--auth-stdin",
    "--output",
    "json",
    "--yes",
    "operator",
    target,
    operation,
    ...(operation === "update" ? [application, revision] : []),
    `--name=${name}`,
    "--owner",
    owner,
    "--status",
    values.CATALOG_STATUS,
  ];
}
function detailOptions(values, target) {
  const application = values.CATALOG_APPLICATION_ID ?? "",
    client = values.CATALOG_CLIENT_ID ?? "";
  if (
    !nonzeroUuid(application) ||
    (target === "client" ? !nonzeroUuid(client) : client !== "") ||
    ["CATALOG_SEARCH", "CATALOG_STATUS", "CATALOG_AFTER", "CATALOG_LIMIT"].some(
      (key) => Boolean(values[key]),
    )
  )
    throw invalid();
  return [application, ...(target === "client" ? [client] : [])];
}
function pageOptions(values, target) {
  const application = values.CATALOG_APPLICATION_ID ?? "";
  if (
    (target === "client" ? !nonzeroUuid(application) : application !== "") ||
    values.CATALOG_CLIENT_ID
  )
    throw invalid();
  return [
    ...(target === "client" ? [application] : []),
    ...listingOptions({
      search: values.CATALOG_SEARCH,
      status: values.CATALOG_STATUS,
      after: values.CATALOG_AFTER,
      limit: values.CATALOG_LIMIT || undefined,
    }),
  ];
}
