import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

const sleepObservation =
  "SELECT count(*) FROM pg_stat_activity WHERE usename='darkhorse_runtime' AND application_name='darkhorse' AND wait_event_type='Timeout' AND wait_event='PgSleep';";

export async function killComposeAccountMutationDuringAudit({
  compose,
  sql,
  stack,
  settings,
  auth,
  signal,
}) {
  const target = `20000000-0000-4000-8000-${randomBytes(6).toString("hex")}`;
  await sql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','${target}@example.com','Interrupted','Mutation');`,
  );
  await installPauseTrigger(sql, target);

  const child = spawn(
    process.execPath,
    ["scripts/account.mjs", "compose-exec"],
    {
      env: {
        ...process.env,
        ...settings,
        ACCOUNT_ID: target,
        ACCOUNT_OPERATION: "revoke-all",
        ACCOUNT_REVISION: "0",
        ACCOUNT_CONFIRM: "yes",
      },
      stdio: ["pipe", "ignore", "ignore"],
    },
  );
  child.stdin.on("error", () => {});
  child.stdin.end(JSON.stringify(auth));
  const closed = new Promise((resolve) => {
    child.once("close", (code, childSignal) => resolve({ code, childSignal }));
  });

  let apiStopped = false;
  try {
    await waitForAuditPause(sql, closed, signal);
    const stopped = await compose(
      stack,
      ["kill", "--signal", "SIGKILL", "api"],
      { capture: true, signal },
    );
    assert.equal(stopped.code, 0, stopped.stderr);
    apiStopped = true;
    const result = await settle(closed, child);
    assert.notEqual(result.code, 0, "the interrupted CLI must fail");
    assert.equal(result.childSignal, null);
  } finally {
    if (!apiStopped && child.exitCode === null && child.signalCode === null)
      await compose(stack, ["kill", "--signal", "SIGKILL", "api"], {
        capture: true,
        signal: undefined,
        acceptFailure: true,
      });
    if (child.exitCode === null && child.signalCode === null)
      child.kill("SIGTERM");
    await settle(closed, child);
    await compose(
      stack,
      ["up", "--detach", "--wait", "--wait-timeout", "60", "api"],
      { capture: true, signal: undefined },
    );
    await removePauseTrigger(sql);
  }

  assert.equal(
    (
      await sql(`SELECT revision FROM principals WHERE id='${target}';`)
    ).stdout.trim(),
    "0",
    "terminating the serving container rolls back the account mutation",
  );
  assert.equal(
    (
      await sql(
        `SELECT count(*) FROM operator_account_audit WHERE target_id='${target}';`,
      )
    ).stdout.trim(),
    "0",
    "a rolled-back mutation leaves no partial audit",
  );
}

async function installPauseTrigger(sql, target) {
  await sql(
    `CREATE FUNCTION darkhorse_test_pause_account_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.target_id='${target}' AND NEW.command='account.revoke_all' THEN PERFORM pg_sleep(4); END IF; RETURN NEW; END $$; CREATE TRIGGER darkhorse_test_pause_account_audit BEFORE INSERT ON operator_account_audit FOR EACH ROW EXECUTE FUNCTION darkhorse_test_pause_account_audit();`,
  );
}

async function removePauseTrigger(sql) {
  await sql(
    "DROP TRIGGER IF EXISTS darkhorse_test_pause_account_audit ON operator_account_audit; DROP FUNCTION IF EXISTS darkhorse_test_pause_account_audit();",
  );
}

async function waitForAuditPause(sql, closed, signal) {
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    const result = await Promise.race([
      sql(sleepObservation),
      closed.then(() => null),
    ]);
    if (result === null)
      throw new Error("Compose account command exited before its audit pause.");
    if (result.stdout.trim() === "1") return;
    await delay(40, undefined, { signal });
  }
  throw new Error("Compose account audit did not reach the controlled pause.");
}

async function settle(closed, child) {
  let timeout;
  const result = await Promise.race([
    closed,
    new Promise((resolve) => {
      timeout = setTimeout(() => resolve(undefined), 10000);
    }),
  ]).finally(() => clearTimeout(timeout));
  if (result) return result;
  if (child.exitCode === null && child.signalCode === null)
    child.kill("SIGKILL");
  throw new Error("Interrupted Compose account launcher did not terminate.");
}
