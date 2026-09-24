export class UsageError extends Error {}
export function protectedStdin(stdinIsTTY) {
  if (stdinIsTTY)
    throw new UsageError(
      "Administration launchers require protected stdin; use a protected file or pipe. Use the documented native terminal commands for hidden interactive input.",
    );
}
export function nonzeroUuid(value) {
  return (
    /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(value ?? "") &&
    value !== "00000000-0000-0000-0000-000000000000"
  );
}
export function listingOptions({
  search = "",
  status = "",
  after = "",
  limit = "25",
}) {
  if (
    !["", "active", "inactive"].includes(status) ||
    !/^(?:[1-9]|1[0-9]|2[0-5])$/.test(limit) ||
    search !== search.trim() ||
    Array.from(search).length > 100 ||
    /[\p{Cc}]/u.test(search) ||
    (after && !nonzeroUuid(after))
  )
    throw new UsageError(
      "Use a literal search prefix of at most 100 characters, active/inactive status, nonzero continuation UUID and a page size from 1 to 25.",
    );
  return [
    "--limit",
    limit,
    ...(search ? [`--search=${search}`] : []),
    ...(status ? ["--status", status] : []),
    ...(after ? ["--after", after] : []),
  ];
}
export function hasCatalogSelectors(values) {
  return [
    "CATALOG_TARGET",
    "CATALOG_OPERATION",
    "CATALOG_APPLICATION_ID",
    "CATALOG_CLIENT_ID",
    "CATALOG_SEARCH",
    "CATALOG_STATUS",
    "CATALOG_AFTER",
    "CATALOG_LIMIT",
    "CATALOG_CONFIRM",
    "CATALOG_REVISION",
    "CATALOG_NAME",
    "CATALOG_OWNER_ID",
  ].some((key) => Boolean(values[key]));
}
