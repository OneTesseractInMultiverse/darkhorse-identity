// Disposable benchmark database only. Setup and observations keep owner access;
// all measured HTTP and native commands use the published runtime grants.
import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";

export async function restrictBenchmarkDatabase(docker, db, databaseUrl) {
  const password = randomBytes(32).toString("hex");
  const url = new URL(databaseUrl);
  assert.equal(url.pathname, "/browser_test");
  const sql = (input) =>
    docker(
      [
        "exec",
        "-i",
        db.name,
        "psql",
        "-U",
        "postgres",
        "-d",
        "browser_test",
        "-v",
        "ON_ERROR_STOP=1",
      ],
      { input },
    );
  await sql(`CREATE ROLE darkhorse_owner NOLOGIN;
CREATE ROLE darkhorse_runtime LOGIN PASSWORD '${password}';
CREATE ROLE darkhorse_operator NOLOGIN;`);
  await sql(
    await readFile(
      new URL("../../deploy/grant-runtime.sql", import.meta.url),
      "utf8",
    ),
  );
  url.username = "darkhorse_runtime";
  url.password = password;
  return url.href;
}
