// Run from the stopped source database. A failure before COMMIT leaves the
// new database closed to every connection, including the restore administrator.
export const createQuarantine = `CREATE DATABASE quarantine WITH TEMPLATE template0 ALLOW_CONNECTIONS false;
BEGIN;
REVOKE ALL ON DATABASE quarantine FROM PUBLIC, darkhorse_owner, darkhorse_operator, darkhorse_runtime;
ALTER DATABASE quarantine ALLOW_CONNECTIONS true;
COMMIT;`;

export async function assertQuarantineConnectionsDenied(compose, stack) {
  for (const role of [
    "darkhorse_runtime",
    "darkhorse_operator",
    "darkhorse_owner",
  ]) {
    const result = await compose(
      stack,
      [
        "exec",
        "-T",
        "postgres",
        "psql",
        "-X",
        "-w",
        "-U",
        role,
        "-d",
        "quarantine",
        "-Atc",
        "SELECT 1",
      ],
      { capture: true, acceptFailure: true },
    );
    requireConnectionDenial(result, role);
  }
}

function requireConnectionDenial(result, role) {
  if (
    result.code !== 2 ||
    !/FATAL:\s+permission denied for database "quarantine"$/m.test(
      result.stderr,
    )
  ) {
    throw new Error(`Expected quarantine CONNECT denial for ${role}.`);
  }
}
