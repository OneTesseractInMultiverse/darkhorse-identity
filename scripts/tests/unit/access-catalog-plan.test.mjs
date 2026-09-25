import assert from "node:assert/strict";
import test from "node:test";
import { catalogOptions } from "../../lib/catalog-plan.mjs";
import { accountOptions } from "../../lib/account-plan.mjs";
const id = "00000000-0000-0000-0000-000000000001";
test("access catalog selectors require explicit scope and bounded literal search", () => {
  for (const target of ["resource", "scope", "role", "capability"]) {
    const values = {
      CATALOG_TARGET: target,
      CATALOG_APPLICATION_ID: id,
      CATALOG_LIMIT: "1",
      CATALOG_SEARCH: "$(touch nope)%_",
    };
    assert.deepEqual(catalogOptions(values, false), [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      target,
      "list",
      ...(["role", "capability"].includes(target)
        ? ["--application", id]
        : [id]),
      "--limit",
      "1",
      "--search=$(touch nope)%_",
    ]);
    for (const change of [
      { CATALOG_OPERATION: "show" },
      { CATALOG_CLIENT_ID: id },
      { CATALOG_SECRET_ID: id },
      { CATALOG_REVISION: "0" },
      { CATALOG_CONFIRM: "yes" },
      { CATALOG_NAME: "name" },
      { CATALOG_OWNER_ID: id },
      { CATALOG_ALL_DEFINITIONS: "yes" },
    ])
      assert.throws(() => catalogOptions({ ...values, ...change }, false));
    assert.throws(() => catalogOptions(values, true));
    assert.throws(() => catalogOptions({ CATALOG_TARGET: target }, false));
    if (target !== "capability")
      assert.throws(() =>
        catalogOptions({ ...values, CATALOG_STATUS: "active" }, false),
      );
  }
});
test("all definitions is explicit, mutually exclusive and rejected by unrelated commands", () => {
  for (const target of ["role", "capability"]) {
    const values = { CATALOG_TARGET: target, CATALOG_ALL_DEFINITIONS: "yes" };
    assert.deepEqual(catalogOptions(values, false), [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      target,
      "list",
      "--all-definitions",
      "--limit",
      "25",
    ]);
    assert.throws(() =>
      catalogOptions({ ...values, CATALOG_ALL_DEFINITIONS: "true" }, false),
    );
    assert.throws(() =>
      catalogOptions({ ...values, CATALOG_APPLICATION_ID: "bad" }, false),
    );
  }
  assert.ok(
    catalogOptions(
      {
        CATALOG_TARGET: "capability",
        CATALOG_ALL_DEFINITIONS: "yes",
        CATALOG_STATUS: "inactive",
        CATALOG_AFTER: id,
      },
      false,
    ).includes("inactive"),
  );
  for (const target of [
    "application",
    "client",
    "client-secret",
    "resource",
    "scope",
  ])
    assert.throws(() =>
      catalogOptions(
        {
          CATALOG_TARGET: target,
          CATALOG_APPLICATION_ID: target === "application" ? "" : id,
          CATALOG_CLIENT_ID: target === "client-secret" ? id : "",
          CATALOG_ALL_DEFINITIONS: "yes",
        },
        false,
      ),
    );
  assert.throws(() =>
    accountOptions(
      { ACCOUNT_OPERATION: "list", CATALOG_ALL_DEFINITIONS: "yes" },
      false,
    ),
  );
});
