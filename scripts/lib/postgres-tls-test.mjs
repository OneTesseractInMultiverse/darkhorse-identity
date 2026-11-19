// Real TLS/process evidence; invoked only against the owned disposable database.
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { createCertificates } from "./deployment-tls.mjs";

export async function verifyPostgresTls(
  docker,
  database,
  command,
  executable,
  environment,
) {
  const directory = await mkdtemp(resolve(".local/postgres-tls-test-"));
  const fixtureCommand = async (args, options = {}) => {
    const result = await docker(args, { ...options, acceptFailure: true });
    assert.equal(
      result.code,
      0,
      `PostgreSQL TLS fixture ${args[0]} failed: ${result.stderr.slice(0, 2048)}`,
    );
    return result;
  };
  const sql = (input) =>
    fixtureCommand(
      [
        "exec",
        "--interactive",
        database,
        "psql",
        "-X",
        "-U",
        "postgres",
        "-d",
        "postgres",
        "-At",
        "-v",
        "ON_ERROR_STOP=1",
      ],
      { input },
    );
  const invoke = (env) =>
    command(executable, ["migrate", "--yes"], {
      env,
      input: "",
      capture: true,
      acceptFailure: true,
    });
  try {
    await createCertificates(directory, "localhost", command);
    for (const [source, destination] of [
      ["edge.pem", "server.crt"],
      ["edge.key", "server.key"],
    ]) {
      // Write through the running process so the file belongs to its tmpfs,
      // rather than the image filesystem beneath that mount. Never log input.
      await fixtureCommand(
        [
          "exec",
          "--interactive",
          "--user",
          "26:26",
          database,
          "sh",
          "-c",
          'umask 077; cat > "$1"',
          "fixture",
          `/data/db/${destination}`,
        ],
        { input: await readFile(join(directory, source)) },
      );
    }
    await sql(
      "ALTER SYSTEM SET ssl_cert_file='/data/db/server.crt';\nALTER SYSTEM SET ssl_key_file='/data/db/server.key';\nALTER SYSTEM SET ssl=on;\nSELECT pg_reload_conf();\n",
    );
    for (let attempt = 0; ; attempt++) {
      if ((await sql("SHOW ssl;\n")).stdout.trim() === "on") break;
      assert.ok(
        attempt < 20,
        "Disposable PostgreSQL TLS configuration must reload",
      );
      await delay(100);
    }
    const trustedUrl = new URL(environment.DARKHORSE_DATABASE_URL);
    trustedUrl.hostname = "localhost";
    const trusted = {
      ...environment,
      DARKHORSE_DATABASE_URL: trustedUrl.toString(),
      DARKHORSE_DATABASE_INSECURE: "false",
      PGSSLROOTCERT: join(directory, "ca.pem"),
    };
    assert.equal(
      (await invoke(trusted)).code,
      0,
      "VerifyFull must accept trusted matching certificate",
    );
    for (const env of [
      {
        ...trusted,
        DARKHORSE_DATABASE_URL: environment.DARKHORSE_DATABASE_URL,
      },
      { ...trusted, PGSSLROOTCERT: join(directory, "postgres.pem") },
    ]) {
      const rejected = await invoke(env);
      assert.notEqual(
        rejected.code,
        0,
        "VerifyFull must reject wrong host or untrusted issuer",
      );
      assert.ok(!rejected.stderr.includes(env.DARKHORSE_DATABASE_URL));
      assert.ok(!rejected.stderr.includes(env.PGSSLROOTCERT));
    }
    assert.equal(
      (await invoke(trusted)).code,
      0,
      "A fresh trusted pool must reconnect after rejected peers",
    );
    console.log(
      "PostgreSQL VerifyFull: trusted connection/reconnect and wrong-host/untrusted-issuer rejection passed.",
    );
  } finally {
    try {
      await sql("ALTER SYSTEM SET ssl=off;\nSELECT pg_reload_conf();\n");
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  }
}
