import {
  UsageError,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
export function accessCatalogOptions(values, target, operation) {
  const application = values.CATALOG_APPLICATION_ID || "";
  const all = values.CATALOG_ALL_DEFINITIONS || "";
  const definitions = ["role", "capability"].includes(target);
  if (
    !["resource", "scope", "role", "capability"].includes(target) ||
    operation !== "list" ||
    [
      "CATALOG_CLIENT_ID",
      "CATALOG_SECRET_ID",
      "CATALOG_CONFIRM",
      "CATALOG_REVISION",
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
    ].some((key) => Boolean(values[key])) ||
    (target !== "capability" && values.CATALOG_STATUS) ||
    (definitions
      ? !((nonzeroUuid(application) && !all) || (!application && all === "yes"))
      : !nonzeroUuid(application) || Boolean(all))
  )
    throw new UsageError(
      "Access catalog listing requires an application, or explicit CATALOG_ALL_DEFINITIONS=yes for roles/capabilities. Status applies only to capabilities. See docs/operator-access-catalog.md.",
    );
  return [
    "--auth-stdin",
    "--output",
    "json",
    "operator",
    target,
    "list",
    ...(definitions
      ? all
        ? ["--all-definitions"]
        : ["--application", application]
      : [application]),
    ...listingOptions({
      search: values.CATALOG_SEARCH,
      status: values.CATALOG_STATUS,
      after: values.CATALOG_AFTER,
      limit: values.CATALOG_LIMIT || undefined,
    }),
  ];
}
