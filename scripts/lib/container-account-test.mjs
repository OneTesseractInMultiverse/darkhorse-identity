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
