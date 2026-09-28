import {
  UsageError,
  nonzeroUuid,
  listingOptions,
} from "./operator-options.mjs";
const invalid = () =>
  new UsageError(
    "Client secret operations require scoped application and client identifiers. Retirement requires a secret identifier; rotation requires a current revision, overlap, confirmation and explicit one-time stdout delivery. See docs/operator-client-secrets.md.",
  );
export function clientSecretOptions(values, operation) {
  const application = values.CATALOG_APPLICATION_ID,
    client = values.CATALOG_CLIENT_ID;
  if (
    !nonzeroUuid(application) ||
    !nonzeroUuid(client) ||
    [
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
      "CATALOG_SEARCH",
      "CATALOG_STATUS",
    ].some((key) => Boolean(values[key]))
  )
    throw invalid();
  const prefix = ["--auth-stdin", "--output", "json"];
  const command = [
    "operator",
    "client",
    "secret",
    operation,
    application,
    client,
  ];
  if (operation === "list") {
    if (
      [
        "CATALOG_SECRET_ID",
        "CATALOG_REVISION",
        "CATALOG_CONFIRM",
        "CATALOG_OVERLAP_SECONDS",
        "CATALOG_SECRET_STDOUT",
      ].some((key) => Boolean(values[key]))
    )
      throw invalid();
    return [
      ...prefix,
      ...command,
      ...listingOptions({
        after: values.CATALOG_AFTER,
        limit: values.CATALOG_LIMIT || undefined,
      }),
    ];
  }
  const revision = values.CATALOG_REVISION ?? "";
  if (operation === "rotate") {
    const overlap = values.CATALOG_OVERLAP_SECONDS ?? "";
    if (
      values.CATALOG_CONFIRM !== "yes" ||
      values.CATALOG_SECRET_STDOUT !== "yes" ||
      values.CATALOG_SECRET_ID ||
      !/^(0|[1-9][0-9]{0,18})$/.test(revision) ||
      BigInt(revision) >= 9223372036854775807n ||
      !/^(0|[1-9][0-9]{0,2})$/.test(overlap) ||
      Number(overlap) > 300 ||
      values.CATALOG_AFTER ||
      values.CATALOG_LIMIT
    )
      throw invalid();
    return [
      ...prefix,
      "--yes",
      ...command,
      revision,
      "--overlap-seconds",
      overlap,
      "--secret-stdout",
    ];
  }
  if (
    operation !== "retire" ||
    !nonzeroUuid(values.CATALOG_SECRET_ID) ||
    values.CATALOG_CONFIRM !== "yes" ||
    !/^(0|[1-9][0-9]{0,18})$/.test(revision) ||
    BigInt(revision) > 9223372036854775807n ||
    values.CATALOG_AFTER ||
    values.CATALOG_LIMIT ||
    values.CATALOG_OVERLAP_SECONDS ||
    values.CATALOG_SECRET_STDOUT
  )
    throw invalid();
  return [...prefix, "--yes", ...command, values.CATALOG_SECRET_ID, revision];
}
