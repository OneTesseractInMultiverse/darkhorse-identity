import {
  UsageError,
  protectedStdin,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
const invalid = () =>
  new UsageError(
    "Select an application/client list or show operation with the required scoped identifiers. Show rejects listing selectors. Omit account, revision and confirmation selectors. See docs/operator-catalog.md.",
  );
export function catalogOptions(values, stdinIsTTY) {
  protectedStdin(stdinIsTTY);
  const target = values.CATALOG_TARGET,
    operation = values.CATALOG_OPERATION || "list";
  if (
    !["application", "client"].includes(target) ||
    !["list", "show"].includes(operation) ||
    [
      "ACCOUNT_OPERATION",
      "ACCOUNT_ID",
      "ACCOUNT_REVISION",
      "ACCOUNT_CONFIRM",
      "ACCOUNT_SEARCH",
      "ACCOUNT_STATUS",
      "ACCOUNT_AFTER",
      "ACCOUNT_LIMIT",
      "CATALOG_CONFIRM",
      "CATALOG_REVISION",
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
