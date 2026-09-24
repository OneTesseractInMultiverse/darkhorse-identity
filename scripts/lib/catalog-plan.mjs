import {
  UsageError,
  protectedStdin,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
export function catalogOptions(values, stdinIsTTY) {
  protectedStdin(stdinIsTTY);
  const target = values.CATALOG_TARGET,
    operation = values.CATALOG_OPERATION || "list",
    application = values.CATALOG_APPLICATION_ID ?? "";
  if (
    operation !== "list" ||
    !["application", "client"].includes(target) ||
    (target === "client" ? !nonzeroUuid(application) : application !== "") ||
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
    throw new UsageError(
      "Select CATALOG_TARGET=application or client and CATALOG_OPERATION=list. Only client listing requires CATALOG_APPLICATION_ID. Omit account, revision and confirmation selectors. See docs/operator-catalog.md.",
    );
  return [
    "--auth-stdin",
    "--output",
    "json",
    "operator",
    target,
    "list",
    ...(target === "client" ? [application] : []),
    ...listingOptions({
      search: values.CATALOG_SEARCH,
      status: values.CATALOG_STATUS,
      after: values.CATALOG_AFTER,
      limit: values.CATALOG_LIMIT || undefined,
    }),
  ];
}
