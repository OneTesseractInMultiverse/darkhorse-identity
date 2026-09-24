import test from "node:test";
import assert from "node:assert/strict";
import { catalogOptions } from "../../lib/catalog-plan.mjs";
import { UsageError } from "../../lib/account-plan.mjs";
const id = "00000000-0000-0000-0000-000000000123";
test("catalog launchers require an explicit target and only expose existing read commands", () => {
  for (const target of ["application", "client"]) {
    const values = {
      CATALOG_TARGET: target,
      ...(target === "client" ? { CATALOG_APPLICATION_ID: id } : {}),
    };
    assert.deepEqual(catalogOptions(values, false), [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      target,
      "list",
      ...(target === "client" ? [id] : []),
      "--limit",
      "25",
    ]);
    assert.deepEqual(
      catalogOptions(
        { ...values, CATALOG_OPERATION: "", CATALOG_LIMIT: "" },
        false,
      ),
      catalogOptions(values, false),
    );
    assert.throws(() => catalogOptions(values, true), /protected stdin/);
    for (const operation of ["create", "rotate-secret", "show", "list --help"])
      assert.throws(
        () =>
          catalogOptions({ ...values, CATALOG_OPERATION: operation }, false),
        UsageError,
      );
  }
  for (const values of [
    {},
    { CATALOG_TARGET: "account" },
    { CATALOG_TARGET: "application", CATALOG_APPLICATION_ID: id },
    { CATALOG_TARGET: "client" },
    {
      CATALOG_TARGET: "client",
      CATALOG_APPLICATION_ID: "00000000-0000-0000-0000-000000000000",
    },
    { CATALOG_TARGET: "client", CATALOG_APPLICATION_ID: "--help" },
  ])
    assert.throws(() => catalogOptions(values, false), UsageError);
});
test("catalog filters preserve literal text and independent continuation without shell expansion", () => {
  const prefix = "--$(touch /tmp/unwanted) `echo x` %_\\";
  assert.deepEqual(
    catalogOptions(
      {
        CATALOG_TARGET: "client",
        CATALOG_OPERATION: "list",
        CATALOG_APPLICATION_ID: id,
        CATALOG_SEARCH: prefix,
        CATALOG_STATUS: "inactive",
        CATALOG_AFTER: id,
        CATALOG_LIMIT: "2",
      },
      false,
    ),
    [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      "client",
      "list",
      id,
      "--limit",
      "2",
      `--search=${prefix}`,
      "--status",
      "inactive",
      "--after",
      id,
    ],
  );
  assert.ok(
    catalogOptions(
      {
        CATALOG_TARGET: "application",
        CATALOG_SEARCH: "😀".repeat(100),
        CATALOG_LIMIT: "1",
      },
      false,
    ).includes(`--search=${"😀".repeat(100)}`),
  );
});
test("catalog selectors reject invalid bounds and cross-command settings without echoing values", () => {
  const marker = "private-invalid-selector";
  for (const change of [
    { CATALOG_TARGET: marker },
    { CATALOG_LIMIT: marker },
    { CATALOG_LIMIT: "0" },
    { CATALOG_LIMIT: "26" },
    { CATALOG_LIMIT: "01" },
    { CATALOG_STATUS: "all" },
    { CATALOG_AFTER: "bad" },
    { CATALOG_AFTER: "00000000-0000-0000-0000-000000000000" },
    { CATALOG_SEARCH: " trim" },
    { CATALOG_SEARCH: "line\nnext" },
    { CATALOG_SEARCH: "😀".repeat(101) },
    ...[
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
    ].map((key) => ({ [key]: marker })),
  ]) {
    assert.throws(
      () => catalogOptions({ CATALOG_TARGET: "application", ...change }, false),
      (error) => error instanceof UsageError && !error.message.includes(marker),
    );
  }
  assert.doesNotThrow(() =>
    catalogOptions(
      { CATALOG_TARGET: "application", ACCOUNT_POD: "reviewed-pod" },
      false,
    ),
  );
});
