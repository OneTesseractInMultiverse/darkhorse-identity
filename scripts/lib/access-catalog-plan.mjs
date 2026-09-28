import {
  UsageError,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
export function accessCatalogOptions(values, target, operation) {
  const application = values.CATALOG_APPLICATION_ID || "";
  const targetId = values.CATALOG_TARGET_ID || "";
  const resourceId = values.CATALOG_RESOURCE_ID || "";
  const all = values.CATALOG_ALL_DEFINITIONS || "";
  const definitions = ["role", "capability"].includes(target);
  if (
    !["resource", "scope", "role", "capability"].includes(target) ||
    !["list", "show"].includes(operation) ||
    [
      "CATALOG_CLIENT_ID",
      "CATALOG_SECRET_ID",
      "CATALOG_CONFIRM",
      "CATALOG_REVISION",
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
    ].some((key) => Boolean(values[key])) ||
    (target !== "capability" && values.CATALOG_STATUS)
  )
    throw new UsageError(
      "Access catalog commands require exact target identifiers and an explicit definition scope. Status applies only to capability listings. See docs/operator-access-catalog.md.",
    );
  if (operation === "show") {
    if (
      [
        "CATALOG_SEARCH",
        "CATALOG_STATUS",
        "CATALOG_AFTER",
        "CATALOG_LIMIT",
      ].some((key) => Boolean(values[key])) ||
      !nonzeroUuid(targetId) ||
      (definitions
        ? !(
            (nonzeroUuid(application) && !all) ||
            (!application && all === "yes")
          ) || Boolean(resourceId)
        : !nonzeroUuid(application) ||
          Boolean(all) ||
          (target === "scope" ? !nonzeroUuid(resourceId) : Boolean(resourceId)))
    )
      throw new UsageError(
        "Access catalog details require exact identifiers and an explicit definition scope. See docs/operator-access-catalog.md.",
      );
    return [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      target,
      "show",
      ...(definitions
        ? all
          ? ["--all-definitions", targetId]
          : ["--application", application, targetId]
        : target === "scope"
          ? [application, resourceId, targetId]
          : [application, targetId]),
    ];
  }
  if (
    targetId ||
    resourceId ||
    (definitions
      ? !((nonzeroUuid(application) && !all) || (!application && all === "yes"))
      : !nonzeroUuid(application) || Boolean(all))
  )
    throw new UsageError(
      "Access catalog listing requires an application, or explicit CATALOG_ALL_DEFINITIONS=yes for roles/capabilities. See docs/operator-access-catalog.md.",
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
