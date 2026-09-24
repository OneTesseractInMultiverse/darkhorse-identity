import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { accountCommand, accountResult } from "./container-account-test.mjs";
import { accountPod, removal } from "./kubernetes-account-plan.mjs";
import { databaseSignal } from "./kubernetes-isolation-test.mjs";

export async function stoppedKubernetesAccounts({
  c,
  kube,
  apply,
  sql,
  command,
  settings,
  auth,
  user,
}) {
  await kube([
    "-n",
    c.namespace,
    "scale",
    "deployment/darkhorse",
    "--replicas=0",
  ]);
  await kube([
    "-n",
    c.namespace,
    "wait",
    "--for=delete",
    "pod",
    "-l",
    "app.kubernetes.io/component=server",
    "--timeout=60s",
  ]);
  accountResult(await accountCommand(command, "kube-exec", settings, auth), 1);
  const target = "00000000-0000-0000-0000-000000000009";
  await sql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','one-shot@example.com','One-shot','Fixture');`,
  );
  const accountFor =
    (credential) =>
    (extra = {}, input = credential) =>
      accountCommand(
        command,
        "kube-run",
        { ...settings, ACCOUNT_ID: target, ...extra },
        input,
      );
  await accountPodIsolation({ c, kube, apply });
  const rejected = await fixtureAdministrator(sql, user, auth);
  await rejectedAccounts(accountFor(rejected.auth), rejected.auth);
  const changes = await fixtureAdministrator(sql, user, auth);
  await changedAccounts(accountFor(changes.auth), target, sql);
  const operator = await fixtureAdministrator(sql, user, auth);
  const account = accountFor(operator.auth);
  const made = await command(
    "make",
    [
      "--no-print-directory",
      "kube-account-run",
      ...Object.entries({
        ...settings,
        ACCOUNT_ID: target,
        ACCOUNT_OPERATION: "show",
        ACCOUNT_REVISION: "",
        ACCOUNT_CONFIRM: "no",
      }).map(([k, v]) => `${k}=${v}`),
    ],
    { input: JSON.stringify(operator.auth) },
  );
  assert.equal(accountResult(made, 0).revision, 3);
  assert.ok(
    !made.stdout.includes(auth.password) &&
      !made.stderr.includes(auth.password),
  );
  await unavailableAccount({ c, kube, account });
  await sql(
    `DELETE FROM platform_administrators WHERE principal_id='${operator.principal}';`,
  );
  accountResult(
    await account(),
    1,
    /Administrator authentication or authority denied/,
  );
  const pods = JSON.parse(
    (await kube(["-n", c.namespace, "get", "pods", "-o", "json"])).stdout,
  ).items;
  assert.ok(
    pods.every(
      (p) =>
        !["server", "account"].includes(
          p.metadata.labels?.["app.kubernetes.io/component"],
        ),
    ),
  );
  console.log(
    "One-shot Kubernetes account commands passed with zero serving Pods: lifecycle mutations/audit, authority and dependency failures, secret/network isolation and UID-conditioned cleanup.",
  );
}
async function rejectedAccounts(account, auth) {
  accountResult(
    await account(
      {},
      {
        email: auth.email,
        password: "source-defined-wrong-password",
      },
    ),
    1,
    /Administrator authentication or authority denied/,
  );
  accountResult(
    await account({ ACCOUNT_OPERATION: "revoke-all", ACCOUNT_REVISION: "0" }),
    3,
    /confirmation_required/,
  );
  accountResult(
    await account({
      ACCOUNT_OPERATION: "revoke-all",
      ACCOUNT_REVISION: "99",
      ACCOUNT_CONFIRM: "yes",
    }),
    1,
    /principal changed/,
  );
  assert.equal(accountResult(await account(), 0).revision, 0);
}
async function changedAccounts(account, target, sql) {
  for (const [operation, revision] of [
    ["deactivate", 0],
    ["reactivate", 1],
    ["revoke-all", 2],
  ]) {
    const value = accountResult(
      await account({
        ACCOUNT_OPERATION: operation,
        ACCOUNT_REVISION: String(revision),
        ACCOUNT_CONFIRM: "yes",
      }),
      0,
    );
    assert.equal(value.revision, revision + 1);
    assert.equal(value.changed, true);
    const audit = await sql(
      `SELECT count(*) FROM operator_account_audit WHERE operation_id='${value.operation_id}' AND target_id='${target}' AND result='changed' AND target_revision=${revision + 1} AND database_role='darkhorse_runtime';`,
    );
    assert.equal(audit.stdout.trim(), "1");
  }
  const shown = accountResult(await account(), 0);
  assert.equal(shown.revision, 3);
  assert.equal(shown.active, true);
}
async function unavailableAccount({ c, kube, account }) {
  await databaseSignal({ c, kube }, "STOP");
  try {
    accountResult(await account(), 1, /Account operation unavailable/);
  } finally {
    await databaseSignal({ c, kube }, "CONT");
  }
}
async function fixtureAdministrator(sql, user, auth) {
  // Separate scenario actors keep each phase within the real five-attempt budget.
  const replacement = randomUUID(),
    credential = randomUUID();
  const email = `${replacement}@example.com`;
  await sql(`BEGIN;
INSERT INTO principals(id,email,first_name,last_name) VALUES('${replacement}','${email}','Account','Fixture');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${replacement}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${user.principal}' AND NOT c.revoked;
INSERT INTO platform_administrators(principal_id) VALUES('${replacement}'); COMMIT;`);
  return { principal: replacement, auth: { ...auth, email } };
}
async function accountPodIsolation({ c, kube, apply }) {
  const manifest = accountPod(c, "darkhorse-account-0123456789abcdef");
  const name = manifest.metadata.name;
  await apply(manifest);
  try {
    await kube([
      "-n",
      c.namespace,
      "wait",
      "--for=condition=Ready",
      `pod/${name}`,
      "--timeout=60s",
    ]);
    const exec = (args, options = {}) =>
      kube(
        ["-n", c.namespace, "exec", name, "-c", "api", "--", ...args],
        options,
      );
    await exec([
      "sh",
      "-c",
      "test ! -e /run/secrets/wrap-key && test ! -e /run/secrets/owner-db && test ! -e /run/secrets/operator-db && test ! -e /run/secrets/limiter-admin-url && test ! -e /var/run/secrets/kubernetes.io/serviceaccount/token && test $(id -u) = 10001",
    ]);
    assert.notEqual(
      (
        await exec(["sh", "-c", "touch /app/forbidden"], {
          acceptFailure: true,
        })
      ).code,
      0,
    );
    assert.notEqual(
      (
        await exec(["darkhorse-server", "migrate", "--yes"], {
          acceptFailure: true,
        })
      ).code,
      0,
    );
    for (const [host, port] of [
      ["postgres", 5432],
      ["limiter", 6379],
    ]) {
      const r = await exec(
        ["timeout", "3", "bash", "-c", `exec 3<>/dev/tcp/${host}/${port}`],
        { acceptFailure: true },
      );
      assert.equal(r.code, 0, r.stderr);
    }
    await steadyCacheDenial(exec);
    const wrong = removal(manifest, "00000000-0000-0000-0000-000000000001");
    const rejected = await kube(
      ["-n", c.namespace, "delete", "--raw", wrong.path, "-f", "-"],
      { input: JSON.stringify(wrong.body), acceptFailure: true },
    );
    assert.notEqual(rejected.code, 0);
    assert.match(rejected.stderr, /Precondition|precondition|Conflict/);
    await kube(["-n", c.namespace, "get", `pod/${name}`]);
  } finally {
    await kube([
      "-n",
      c.namespace,
      "delete",
      `pod/${name}`,
      "--wait=true",
      "--timeout=30s",
    ]);
  }
}
async function steadyCacheDenial(exec) {
  // Observe policy convergence with read-only TCP probes. Never retry an account command.
  let admitted = 0,
    denied = 0;
  for (let n = 0; n < 30 && denied < 2; n++) {
    const r = await exec(
      ["timeout", "3", "bash", "-c", "exec 3<>/dev/tcp/cache/6379"],
      { acceptFailure: true },
    );
    assert.ok([0, 124].includes(r.code), r.stderr);
    if (r.code === 124) denied++;
    else {
      admitted++;
      denied = 0;
    }
    if (denied < 2) await delay(500);
  }
  console.log(
    `Account policy probe: ${admitted} connections admitted before two consecutive denials. Startup isolation is not qualified.`,
  );
  assert.equal(denied, 2, "account policy must converge to cache denial");
}
