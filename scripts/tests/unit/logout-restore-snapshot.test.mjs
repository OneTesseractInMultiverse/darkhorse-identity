import test from "node:test";
import assert from "node:assert/strict";
import {
  parseLogoutSnapshot,
  assertLogoutRestore,
  assertLogoutArchive,
} from "../../lib/logout-restore-snapshot.mjs";

const id = (n) => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
// All facts are source-defined. No files, SQL, tokens, clock or services are used.
function fixture() {
  return {
    logout_account_intents: [
      ["2", id(1), "0", "1000", id(12), "true"],
      ["5", id(2), "0", "4000", null, "false"],
    ],
    credential_logout_events: [
      ["3", id(3), id(1), "0", "2000", "fixture_role"],
      ["4", id(4), id(2), "0", "3000", "fixture_role"],
    ],
    logout_credential_intents: [
      ["3", "2000", null, "false"],
      ["4", "3000", id(13), "true"],
    ],
    logout_intents: [
      ["7", id(11), "1000", id(21), "true", "2"],
      ["8", id(12), "1000", id(21), "true", "2"],
      ["9", id(13), "3000", null, "false", "4"],
    ],
    logout_deliveries: [
      [id(31), "7", id(41), id(21), "1", "queued", "2", "1000", "61000"],
      [id(32), "8", id(42), id(21), "1", "delivered", "1", "1000", "2000"],
    ],
    logout_destinations: [
      [id(21), "1", "1", "https://restore.invalid/original"],
      [id(21), "2", "6", "https://restore.invalid/replacement"],
    ],
    logout_audit: [
      ["1", id(21), null, null, "1", "configured", "999", "fixture_role"],
      ["2", id(21), null, id(31), "1", "claimed", "1100", "fixture_role"],
      ["3", id(21), null, id(31), "1", "retry", "1200", "fixture_role"],
      ["4", id(21), null, id(31), "1", "claimed", "60000", "fixture_role"],
      ["5", id(21), null, id(32), "1", "claimed", "1100", "fixture_role"],
      ["6", id(21), null, id(32), "1", "delivered", "2000", "fixture_role"],
    ],
    browser_sessions: [
      [id(11), id(1), id(3), "0"],
      [id(12), id(1), id(3), "0"],
      [id(13), id(2), id(4), "0"],
    ],
    credentials: [
      [id(3), id(1), "password", "true"],
      [id(4), id(2), "password", "true"],
    ],
    relying_party_sessions: [
      [
        id(41),
        "https://identity.invalid",
        id(11),
        id(1),
        id(21),
        "100",
        "fixture_role",
      ],
      [
        id(42),
        "https://identity.invalid",
        id(12),
        id(1),
        id(21),
        "100",
        "fixture_role",
      ],
      [
        id(43),
        "https://identity.invalid",
        id(13),
        id(2),
        id(21),
        "100",
        "fixture_role",
      ],
    ],
    sequences: [
      ["logout_order", "9", "true"],
      ["logout_audit_id_seq", "6", "true"],
    ],
  };
}
const encode = (value) => JSON.stringify(Object.entries(value));
const parsed = (value = fixture()) => parseLogoutSnapshot(encode(value));
const reject = (change) => {
  const value = fixture();
  change(value);
  assert.throws(() => parsed(value), /Logout restore/);
};

test("nonempty account-first and credential-first facts survive row reordering", () => {
  const source = parsed();
  const reordered = Object.fromEntries(Object.entries(fixture()).reverse());
  for (const rows of Object.values(reordered)) rows.reverse();
  assert.doesNotThrow(() => assertLogoutRestore(source, parsed(reordered)));
  assert.doesNotThrow(() =>
    assertLogoutRestore(source, parseLogoutSnapshot(encode(fixture()) + "\n")),
  );
});

test("empty, omitted, duplicate and unexpected categories cannot pass a restore", () => {
  for (const name of Object.keys(fixture())) {
    reject((value) => {
      delete value[name];
    });
    reject((value) => {
      value[name] = [];
    });
    reject((value) => {
      value[name].push([...value[name][0]]);
    });
  }
  reject((value) => {
    value.extra = [];
  });
  const entries = Object.entries(fixture());
  assert.throws(
    () => parseLogoutSnapshot(JSON.stringify([...entries, entries[0]])),
    /Logout restore/,
  );
});

test("parser rejects malformed, oversized, unsafe numeric and secret-bearing shapes without echoing rows", () => {
  for (const text of [
    "{",
    "null",
    "{}",
    "[]",
    " ".repeat(2 * 1024 * 1024 + 1),
    "BEGIN\n" + encode(fixture()) + "\nCOMMIT",
    encode(fixture()) + "\n[]",
  ])
    assert.throws(() => parseLogoutSnapshot(text), /^Error: Logout restore/);
  reject((v) => {
    v.logout_intents[0][0] = 7;
  });
  for (const value of ["01", "-1", "1e2", "9223372036854775808"])
    reject((v) => {
      v.logout_intents[0][0] = value;
    });
  reject((v) => {
    v.logout_intents[0][4] = "yes";
  });
  reject((v) => {
    v.logout_intents[0].push("private-fixture-value");
  });
  reject((v) => {
    v.browser_sessions[0][0] = "not-a-uuid";
  });
  reject((v) => {
    v.logout_deliveries[0][6] = "9";
  });
  reject((v) => {
    v.logout_deliveries[0][8] = "999";
  });
  reject((v) => {
    v.logout_deliveries[0][5] = "sending";
  });
  reject((v) => {
    v.logout_destinations[0][3] = "http://restore.invalid/";
  });
  reject((v) => {
    v.logout_audit = Array.from({ length: 1025 }, (_, i) => [
      String(i + 1),
      ...v.logout_audit[0].slice(1),
    ]);
  });
  try {
    parseLogoutSnapshot('["private-fixture-value"]');
    assert.fail("must reject malformed input");
  } catch (error) {
    assert.ok(!error.message.includes("private-fixture-value"));
  }
});

test("bigint identities retain precision beyond JavaScript's safe integer range", () => {
  const value = fixture();
  const offset = 9007199254740992n;
  for (const [table, column] of [
    ["logout_account_intents", 0],
    ["credential_logout_events", 0],
    ["logout_credential_intents", 0],
    ["logout_intents", 0],
    ["logout_intents", 5],
    ["logout_deliveries", 1],
    ["logout_destinations", 2],
  ])
    for (const row of value[table])
      row[column] = String(BigInt(row[column]) + offset);
  value.sequences[0][1] = String(9n + offset);
  assert.doesNotThrow(() => assertLogoutRestore(parsed(value), parsed(value)));
});

test("copied source ordering, time, credential ownership and cursor bindings must agree", () => {
  for (const change of [
    (v) => {
      v.logout_credential_intents[0][1] = "2001";
    },
    (v) => {
      v.logout_credential_intents[0][0] = "99";
    },
    (v) => {
      v.credential_logout_events[0][2] = id(2);
    },
    (v) => {
      v.credentials[0][3] = "false";
    },
    (v) => {
      v.logout_intents[0][5] = "3";
      v.logout_intents[0][2] = "2000";
    },
    (v) => {
      v.logout_intents[2][5] = "5";
      v.logout_intents[2][2] = "4000";
    },
    (v) => {
      v.logout_intents[0][2] = "1001";
    },
    (v) => {
      v.logout_intents[0][5] = "7";
    },
    (v) => {
      v.logout_account_intents[0][4] = id(13);
    },
    (v) => {
      v.logout_credential_intents[1][2] = id(11);
    },
    (v) => {
      v.logout_intents[0][3] = id(99);
    },
    (v) => {
      v.sequences[0][1] = "8";
    },
  ])
    reject(change);
});

test("queue coverage must include completed cursors and pending roots, not just nonempty tables", () => {
  for (const [table, cursor, expanded] of [
    ["logout_account_intents", 4, 5],
    ["logout_credential_intents", 2, 3],
    ["logout_intents", 3, 4],
  ]) {
    reject((value) => {
      for (const row of value[table]) {
        row[cursor] = null;
        row[expanded] = "false";
      }
    });
    reject((value) => {
      for (const row of value[table]) row[expanded] = "true";
    });
  }
});

test("deliveries retain original destinations, RP bindings, timestamps and audit linkage", () => {
  for (const change of [
    (v) => {
      v.logout_deliveries[0][4] = "2";
    },
    (v) => {
      v.logout_deliveries[0][2] = id(42);
    },
    (v) => {
      v.logout_deliveries[0][3] = id(22);
    },
    (v) => {
      v.logout_deliveries[0][7] = "1001";
    },
    (v) => {
      v.logout_audit[1][1] = id(22);
    },
    (v) => {
      v.logout_audit[1][4] = "2";
    },
    (v) => {
      v.relying_party_sessions[0][3] = id(2);
    },
  ])
    reject(change);
});

test("quarantined comparison rejects valid-looking immutable, binding and progress drift", () => {
  const source = parsed();
  for (const change of [
    (v) => {
      v.logout_destinations[0][3] = "https://restore.invalid/corrupted";
    },
    (v) => {
      v.logout_audit[0][7] = "changed_role";
    },
    (v) => {
      v.credential_logout_events[0][5] = "changed_role";
    },
    (v) => {
      v.logout_credential_intents[0][3] = "true";
    },
    (v) => {
      v.logout_account_intents[1][5] = "true";
    },
    (v) => {
      v.logout_intents[2][4] = "true";
    },
    (v) => {
      v.logout_deliveries[0][6] = "3";
    },
    (v) => {
      v.logout_deliveries[0][8] = "62000";
    },
    (v) => {
      v.logout_deliveries[1][5] = "failed";
    },
    (v) => {
      v.sequences[0][1] = "10";
    },
  ]) {
    const restored = fixture();
    change(restored);
    assert.throws(
      () => assertLogoutRestore(source, parsed(restored)),
      /Logout restore/,
    );
  }
});

const tables = Object.keys(fixture()).slice(0, 7);
const inventory = () =>
  tables
    .flatMap((name, i) => [
      `${100 + i}; 1259 ${200 + i} TABLE public ${name} postgres`,
      `${300 + i}; 0 ${200 + i} TABLE DATA public ${name} postgres`,
    ])
    .concat([
      "400; 0 0 SEQUENCE SET public logout_order postgres",
      "401; 0 0 SEQUENCE SET public logout_audit_id_seq postgres",
    ])
    .join("\n");

test("archive inventory requires exact public table data and sequence-state entries", () => {
  assert.doesNotThrow(() => assertLogoutArchive(inventory()));
  for (const name of tables) {
    assert.throws(
      () =>
        assertLogoutArchive(
          inventory().replace(
            `TABLE DATA public ${name} `,
            `TABLE DATA public ${name}_other `,
          ),
        ),
      /Logout restore/,
    );
    assert.throws(
      () =>
        assertLogoutArchive(
          inventory().replace(
            `TABLE public ${name} `,
            `TABLE private ${name} `,
          ),
        ),
      /Logout restore/,
    );
  }
  assert.throws(
    () =>
      assertLogoutArchive(
        inventory().replace(
          "SEQUENCE SET public logout_order",
          "SEQUENCE public logout_order",
        ),
      ),
    /Logout restore/,
  );
  assert.throws(
    () => assertLogoutArchive(inventory() + "\n" + inventory()),
    /Logout restore/,
  );
});
