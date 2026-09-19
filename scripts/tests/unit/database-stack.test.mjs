import assert from "node:assert/strict";
import { test } from "node:test";
import { checkDatabaseVolumes } from "../../lib/database-stack.mjs";

test("an upstream database cannot silently become a new empty Percona cluster", () => {
  assert.throws(
    () =>
      checkDatabaseVolumes(["darkhorse-unit_database-data"], "darkhorse-unit"),
    /Migrate the existing PostgreSQL volume/,
  );
  assert.doesNotThrow(() => checkDatabaseVolumes([], "darkhorse-unit"));
  assert.doesNotThrow(() =>
    checkDatabaseVolumes(["another_database-data"], "darkhorse-unit"),
  );
  assert.doesNotThrow(() =>
    checkDatabaseVolumes(
      ["darkhorse-unit_database-data", "darkhorse-unit_percona-data"],
      "darkhorse-unit",
    ),
  );
});
