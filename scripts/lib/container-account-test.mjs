import assert from "node:assert/strict";
export async function accountCommand(command, mode, settings, input) {
  const result = await command(
    process.execPath,
    ["scripts/account.mjs", mode],
    {
      env: {
        ...process.env,
        ACCOUNT_OPERATION: "show",
        ACCOUNT_REVISION: "",
        ACCOUNT_CONFIRM: "no",
        ACCOUNT_SEARCH: "",
        ACCOUNT_STATUS: "",
        ACCOUNT_AFTER: "",
        ACCOUNT_LIMIT: "25",
        ...settings,
      },
      input: JSON.stringify(input),
      capture: true,
      acceptFailure: true,
    },
  );
  assert.ok(!result.stdout.includes(input.password));
  assert.ok(!result.stderr.includes(input.password));
  return result;
}
export function accountResult(result, code, failure) {
  assert.equal(result.code, code, result.stderr);
  if (code === 0) {
    const value = JSON.parse(result.stdout);
    assert.equal(value.ok, true);
    assert.equal(value.schema_version, 1);
    return value.data;
  }
  assert.equal(result.stdout, "");
  if (failure) assert.match(result.stderr, failure);
}

export async function directoryPage(
  command,
  mode,
  settings,
  auth,
  target,
  email,
  sql,
) {
  const page = accountResult(
    await accountCommand(
      command,
      mode,
      {
        ...settings,
        ACCOUNT_OPERATION: "list",
        ACCOUNT_ID: "",
        ACCOUNT_REVISION: "",
        ACCOUNT_SEARCH: email,
        ACCOUNT_STATUS: "active",
        ACCOUNT_LIMIT: "1",
      },
      { email: auth.email, password: auth.password },
    ),
    0,
  );
  assert.equal(page.items.length, 1);
  assert.equal(page.items[0].id, target);
  assert.equal(page.next, null);
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_directory_audit WHERE operation_id='${page.operation_id}' AND result='read' AND returned_count=1 AND searched AND database_role='darkhorse_runtime';`,
      )
    ).stdout.trim(),
    "1",
  );
}
