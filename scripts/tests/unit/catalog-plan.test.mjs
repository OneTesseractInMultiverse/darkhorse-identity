import test from "node:test";
import assert from "node:assert/strict";
import { catalogOptions } from "../../lib/catalog-plan.mjs";
import { UsageError } from "../../lib/account-plan.mjs";
const id = "00000000-0000-0000-0000-000000000123";
test("catalog launchers require an explicit target and reject incomplete or unsupported commands", () => {
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
    for (const operation of ["create", "rotate-secret", "list --help"])
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

test("catalog show requires exact scoped identifiers and rejects every listing selector", () => {
  assert.throws(
    () =>
      catalogOptions(
        { CATALOG_TARGET: "application", CATALOG_OPERATION: "show" },
        false,
      ),
    UsageError,
  );
  for (const target of ["application", "client"]) {
    const values = {
      CATALOG_TARGET: target,
      CATALOG_OPERATION: "show",
      CATALOG_APPLICATION_ID: id,
      ...(target === "client" ? { CATALOG_CLIENT_ID: id } : {}),
    };
    assert.deepEqual(catalogOptions(values, false), [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      target,
      "show",
      id,
      ...(target === "client" ? [id] : []),
    ]);
    for (const change of [
      { CATALOG_APPLICATION_ID: "" },
      { CATALOG_APPLICATION_ID: "bad" },
      ...[
        "CATALOG_SEARCH",
        "CATALOG_STATUS",
        "CATALOG_AFTER",
        "CATALOG_LIMIT",
      ].map((key) => ({ [key]: "1" })),
      { CATALOG_CLIENT_ID: target === "client" ? "" : "unexpected" },
    ])
      assert.throws(
        () => catalogOptions({ ...values, ...change }, false),
        UsageError,
      );
  }
  assert.throws(
    () =>
      catalogOptions(
        { CATALOG_TARGET: "application", CATALOG_CLIENT_ID: id },
        false,
      ),
    UsageError,
  );
  assert.throws(
    () =>
      catalogOptions(
        {
          CATALOG_TARGET: "client",
          CATALOG_OPERATION: "show",
          CATALOG_APPLICATION_ID: id,
          CATALOG_CLIENT_ID: "00000000-0000-0000-0000-000000000000",
        },
        false,
      ),
    UsageError,
  );
});

test("application mutations require a complete literal specification and explicit confirmation", () => {
  const name = "--$(touch /tmp/unwanted) `echo x` %_\\";
  const base = {
    CATALOG_TARGET: "application",
    CATALOG_NAME: name,
    CATALOG_OWNER_ID: id,
    CATALOG_STATUS: "inactive",
    CATALOG_CONFIRM: "yes",
  };
  for (const operation of ["create", "update"]) {
    const values = {
      ...base,
      CATALOG_OPERATION: operation,
      ...(operation === "update"
        ? {
            CATALOG_APPLICATION_ID: id,
            CATALOG_REVISION: "9223372036854775807",
          }
        : {}),
    };
    assert.deepEqual(catalogOptions(values, false), [
      "--auth-stdin",
      "--output",
      "json",
      "--yes",
      "operator",
      "application",
      operation,
      ...(operation === "update" ? [id, "9223372036854775807"] : []),
      `--name=${name}`,
      "--owner",
      id,
      "--status",
      "inactive",
    ]);
    for (const field of [
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
      "CATALOG_STATUS",
      "CATALOG_CONFIRM",
    ]) {
      assert.throws(
        () => catalogOptions({ ...values, [field]: "" }, false),
        UsageError,
      );
    }
    for (const change of [
      { CATALOG_CONFIRM: "no" },
      { CATALOG_NAME: "x".repeat(101) },
      { CATALOG_NAME: "private\nmarker" },
      { CATALOG_OWNER_ID: "bad" },
      { CATALOG_STATUS: "all" },
      { CATALOG_CLIENT_ID: id },
      { CATALOG_SEARCH: "x" },
      { CATALOG_LIMIT: "25" },
      { CATALOG_AFTER: id },
      { CATALOG_TARGET: "client" },
      { ACCOUNT_CONFIRM: "yes" },
    ]) {
      assert.throws(
        () => catalogOptions({ ...values, ...change }, false),
        UsageError,
      );
    }
    assert.throws(() => catalogOptions(values, true), /protected stdin/);
  }
  assert.throws(
    () =>
      catalogOptions(
        { ...base, CATALOG_OPERATION: "create", CATALOG_APPLICATION_ID: id },
        false,
      ),
    UsageError,
  );
  assert.throws(
    () =>
      catalogOptions(
        { ...base, CATALOG_OPERATION: "create", CATALOG_REVISION: "0" },
        false,
      ),
    UsageError,
  );
  for (const revision of ["", "-1", "01", "9223372036854775808"]) {
    assert.throws(
      () =>
        catalogOptions(
          {
            ...base,
            CATALOG_OPERATION: "update",
            CATALOG_APPLICATION_ID: id,
            CATALOG_REVISION: revision,
          },
          false,
        ),
      UsageError,
    );
  }
  for (const operation of ["list", "show"])
    for (const key of ["CATALOG_NAME", "CATALOG_OWNER_ID"]) {
      assert.throws(
        () =>
          catalogOptions(
            {
              CATALOG_TARGET: "application",
              CATALOG_OPERATION: operation,
              ...(operation === "show" ? { CATALOG_APPLICATION_ID: id } : {}),
              [key]: "private",
            },
            false,
          ),
        UsageError,
      );
    }
});

test("client updates require scoped IDs revision confirmation and leave configuration in protected stdin", () => {
  const values = {
    CATALOG_TARGET: "client",
    CATALOG_OPERATION: "update",
    CATALOG_APPLICATION_ID: id,
    CATALOG_CLIENT_ID: id,
    CATALOG_REVISION: "0",
    CATALOG_CONFIRM: "yes",
  };
  assert.deepEqual(catalogOptions(values, false), [
    "--auth-stdin",
    "--output",
    "json",
    "--yes",
    "operator",
    "client",
    "update",
    id,
    id,
    "0",
  ]);
  for (const key of [
    "CATALOG_APPLICATION_ID",
    "CATALOG_CLIENT_ID",
    "CATALOG_REVISION",
    "CATALOG_CONFIRM",
  ])
    assert.throws(
      () => catalogOptions({ ...values, [key]: "" }, false),
      UsageError,
    );
  for (const change of [
    { CATALOG_OPERATION: "create" },
    { CATALOG_REVISION: "01" },
    { CATALOG_REVISION: "9223372036854775808" },
    { CATALOG_CLIENT_ID: "bad" },
    ...[
      "CATALOG_NAME",
      "CATALOG_OWNER_ID",
      "CATALOG_STATUS",
      "CATALOG_SEARCH",
      "CATALOG_AFTER",
      "CATALOG_LIMIT",
    ].map((key) => ({ [key]: "private-marker" })),
  ]) {
    assert.throws(
      () => catalogOptions({ ...values, ...change }, false),
      (error) =>
        error instanceof UsageError &&
        !error.message.includes("private-marker"),
    );
  }
  assert.throws(() => catalogOptions(values, true), UsageError);
});

test("client secret inventory and retirement preserve scope and reject conflicting selectors", () => {
  const base = {
    CATALOG_TARGET: "client-secret",
    CATALOG_APPLICATION_ID: id,
    CATALOG_CLIENT_ID: id,
  };
  assert.deepEqual(
    catalogOptions({ ...base, CATALOG_AFTER: id, CATALOG_LIMIT: "2" }, false),
    [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      "client",
      "secret",
      "list",
      id,
      id,
      "--limit",
      "2",
      "--after",
      id,
    ],
  );
  const retire = {
    ...base,
    CATALOG_OPERATION: "retire",
    CATALOG_SECRET_ID: id,
    CATALOG_REVISION: "7",
    CATALOG_CONFIRM: "yes",
  };
  assert.deepEqual(catalogOptions(retire, false), [
    "--auth-stdin",
    "--output",
    "json",
    "--yes",
    "operator",
    "client",
    "secret",
    "retire",
    id,
    id,
    id,
    "7",
  ]);
  for (const bad of [
    { CATALOG_APPLICATION_ID: "" },
    { CATALOG_CLIENT_ID: "" },
    { CATALOG_SECRET_ID: "" },
    { CATALOG_REVISION: "" },
    { CATALOG_REVISION: "9223372036854775808" },
    { CATALOG_CONFIRM: "no" },
    { CATALOG_AFTER: id },
    { CATALOG_LIMIT: "25" },
    { CATALOG_NAME: "Name" },
    { CATALOG_OWNER_ID: id },
    { CATALOG_SEARCH: "query" },
    { CATALOG_STATUS: "active" },
  ])
    assert.throws(
      () => catalogOptions({ ...retire, ...bad }, false),
      UsageError,
    );
  for (const bad of [
    { CATALOG_SECRET_ID: id },
    { CATALOG_REVISION: "0" },
    { CATALOG_CONFIRM: "yes" },
    { CATALOG_SEARCH: "x" },
    { CATALOG_LIMIT: "26" },
    { CATALOG_AFTER: "bad" },
    { CATALOG_OPERATION: "create" },
  ])
    assert.throws(() => catalogOptions({ ...base, ...bad }, false), UsageError);
  for (const target of ["application", "client"])
    assert.throws(
      () =>
        catalogOptions(
          { ...base, CATALOG_TARGET: target, CATALOG_SECRET_ID: id },
          false,
        ),
      UsageError,
    );
});
