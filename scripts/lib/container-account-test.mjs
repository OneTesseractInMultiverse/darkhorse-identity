import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
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

// Execute a real Compose mutation, then discard the successful response at the
// host transport boundary. The caller must reconcile committed state/audit and
// prove that the launcher never retries the operation.
export async function lostComposeAccountResponse(command, settings, auth, sql) {
  const target = `10000000-0000-4000-8000-${randomBytes(6).toString("hex")}`;
  await sql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','${target}@example.com','Response','Lost');`,
  );
  const docker = (
    await command("which", ["docker"], { capture: true })
  ).stdout.trim();
  assert.ok(
    docker.startsWith("/"),
    "Docker executable must resolve absolutely",
  );
  const directory = await mkdtemp(
    join(tmpdir(), "darkhorse-lost-compose-response-"),
  );
  try {
    await writeFile(
      join(directory, "docker"),
      `#!/usr/bin/env node\nimport { spawn } from "node:child_process";\nconst child=spawn(${JSON.stringify(docker)},process.argv.slice(2),{stdio:["inherit","ignore","inherit"]});\nchild.once("error",()=>{process.exitCode=127});\nchild.once("close",code=>{if(code===0)process.kill(process.pid,"SIGTERM");else process.exitCode=code??1});\n`,
      { mode: 0o700 },
    );
    const result = await accountCommand(
      command,
      "compose-exec",
      {
        ...settings,
        PATH: `${directory}:${process.env.PATH}`,
        ACCOUNT_ID: target,
        ACCOUNT_OPERATION: "revoke-all",
        ACCOUNT_REVISION: "0",
        ACCOUNT_CONFIRM: "yes",
      },
      auth,
    );
    assert.equal(result.code, 1);
    assert.equal(result.stdout, "");
    assert.match(
      result.stderr,
      /Administration transport interrupted or timed out.*no retry was attempted/s,
    );
    assert.equal(
      (
        await sql(`SELECT revision FROM principals WHERE id='${target}';`)
      ).stdout.trim(),
      "1",
      "the mutation committed exactly once despite the lost response",
    );
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_account_audit WHERE target_id='${target}' AND command='account.revoke_all' AND expected_revision=0 AND target_revision=1 AND result='changed' AND database_role='darkhorse_runtime';`,
        )
      ).stdout.trim(),
      "1",
      "the single committed runtime-role audit is the recovery evidence",
    );
    assert.equal(
      (
        await sql(
          `SELECT count(*) FROM operator_account_audit WHERE target_id='${target}';`,
        )
      ).stdout.trim(),
      "1",
      "the lost response must not trigger a second audited attempt",
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
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
