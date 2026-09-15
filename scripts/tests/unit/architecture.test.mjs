import { test } from "node:test";
import assert from "node:assert/strict";
import { boundaryErrors } from "../../lib/architecture.mjs";

test("permits inward dependencies and static UI files", () => {
  assert.deepEqual(
    boundaryErrors(
      [
        { name: "darkhorse-domain", dependencies: [] },
        {
          name: "darkhorse-application",
          dependencies: [{ name: "darkhorse-domain" }],
        },
      ],
      ["apps/console/src/routes/+page.svelte"],
    ),
    [],
  );
});

test("rejects renamed, development, and framework dependencies in the core", () => {
  assert.equal(
    boundaryErrors(
      [
        {
          name: "darkhorse-domain",
          dependencies: [{ name: "axum", rename: "http", kind: "dev" }],
        },
        {
          name: "darkhorse-application",
          dependencies: [{ name: "darkhorse-adapters" }],
        },
      ],
      [],
    ).length,
    2,
  );
});

test("rejects Svelte server behavior and colocated test source", () => {
  assert.equal(
    boundaryErrors(
      [],
      [
        "apps/console/src/routes/+server.ts",
        "apps/console/src/routes/+page.server.ts",
        "apps/console/src/lib/server/session.ts",
        "apps/console/src/lib/example.test.ts",
      ],
    ).length,
    4,
  );
});
