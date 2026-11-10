import { test } from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { createLogoutStore } from "../../lib/reference-logout-store.mjs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Worker } from "node:worker_threads";
import { once } from "node:events";
import { run } from "../../lib/command.mjs";

const binding = {
  iss: "https://issuer.example",
  aud: "client",
  sub: "subject",
  sid: "old",
  iat: 1000,
  exp: 1300,
};
const logout = { ...binding, jti: "token", exp: 1120 };
function fixture(t, limit) {
  const db = new DatabaseSync(":memory:");
  let now = 1000;
  t.after(() => db.close());
  return {
    db,
    store: createLogoutStore(db, () => now, limit),
    time: (value) => {
      now = value;
    },
  };
}
test("atomic logout terminates every exact binding and preserves unrelated or newer sessions", (t) => {
  const { store } = fixture(t);
  store.bind("old-1", binding);
  store.bind("old-2", binding);
  for (const [field, value] of Object.entries({
    iss: "https://other.example",
    aud: "other",
    sub: "other",
    sid: "new",
  }))
    store.bind(field, { ...binding, [field]: value });
  assert.equal(store.accept(logout, "a"), "accepted");
  assert.deepEqual(store.sessions(), ["aud", "iss", "sid", "sub"]);
  assert.equal(store.accept(logout, "a"), "duplicate");
  assert.equal(store.accept({ ...logout, sid: "new" }, "different"), "invalid");
  assert.equal(store.accept({ ...logout, jti: "retry" }, "b"), "accepted");
  assert.deepEqual(store.sessions(), ["aud", "iss", "sid", "sub"]);
});
test("storage failure rolls replay, tombstone and session deletion back together", (t) => {
  const { db, store } = fixture(t);
  store.bind("old", binding);
  db.exec(
    "CREATE TRIGGER fail_delete BEFORE DELETE ON rp_sessions BEGIN SELECT RAISE(ABORT, 'injected'); END",
  );
  assert.throws(() => store.accept(logout, "a"));
  assert.deepEqual(store.sessions(), ["old"]);
  assert.equal(db.prepare("SELECT count(*) AS n FROM rp_replays").get().n, 0);
  assert.equal(db.prepare("SELECT count(*) AS n FROM rp_ended").get().n, 0);
  db.exec("DROP TRIGGER fail_delete");
  assert.equal(store.accept(logout, "a"), "accepted");
  assert.deepEqual(store.sessions(), []);
});
test("logout before a callback prevents resurrection, including after replay expiry", (t) => {
  const { store, time } = fixture(t);
  assert.equal(store.accept(logout, "a"), "accepted");
  assert.equal(store.bind("late", binding), false);
  time(1120);
  assert.equal(store.accept(logout, "a"), "invalid");
  assert.equal(store.bind("late", binding), false);
  assert.equal(
    store.bind("fresh", { ...binding, sid: "new", iat: 1120, exp: 1420 }),
    true,
  );
  time(1300);
  assert.equal(store.bind("expired", binding), false);
  assert.deepEqual(store.sessions(), ["fresh"]);
});
test("admission fails closed without evicting live replay or tombstone records", (t) => {
  const { store, db, time } = fixture(t, 1);
  assert.equal(store.accept(logout, "a"), "accepted");
  assert.throws(() => store.accept({ ...logout, jti: "retry" }, "b"));
  assert.equal(store.accept(logout, "a"), "duplicate");
  assert.throws(() => store.accept({ ...logout, jti: "new", sid: "new" }, "c"));
  assert.equal(db.prepare("SELECT count(*) AS n FROM rp_replays").get().n, 1);
  time(1120);
  assert.throws(() =>
    store.accept(
      { ...logout, jti: "new", sid: "new", iat: 1120, exp: 1240 },
      "c",
    ),
  );
  assert.equal(store.bind("late", binding), false);
  time(1300);
  assert.equal(
    store.accept(
      { ...logout, jti: "new", sid: "new", iat: 1300, exp: 1420 },
      "c",
    ),
    "accepted",
  );
});
test("writer rechecks token expiry after validation and refuses stale callbacks", (t) => {
  const { store, time } = fixture(t);
  time(1120);
  assert.equal(store.accept(logout, "a"), "invalid");
  assert.equal(store.bind("too-long", { ...binding, exp: 1600 }), false);
  assert.equal(
    store.bind("future", { ...binding, iat: 1200, exp: 1500 }),
    false,
  );
  time(Number.MAX_SAFE_INTEGER - 120);
  assert.equal(
    store.accept(
      {
        ...logout,
        iat: Number.MAX_SAFE_INTEGER - 120,
        exp: Number.MAX_SAFE_INTEGER,
      },
      "overflow",
    ),
    "invalid",
  );
});

test("callback capacity failure cannot replace an existing session", (t) => {
  const { store } = fixture(t, 1);
  assert.equal(store.bind("one", binding), true);
  assert.throws(() => store.bind("two", { ...binding, sid: "new" }));
  assert.deepEqual(store.sessions(), ["one"]);
  assert.throws(() => store.bind("one", { ...binding, sub: "different" }));
  assert.equal(store.accept(logout, "a"), "accepted");
  assert.deepEqual(store.sessions(), []);
});

async function file(t) {
  const directory = await mkdtemp(join(tmpdir(), "darkhorse-rp-store-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return join(directory, "sessions.sqlite");
}
test("a held writer lock fails within a bounded wait without releasing success", async (t) => {
  const path = await file(t);
  const first = new DatabaseSync(path),
    second = new DatabaseSync(path);
  try {
    const owner = createLogoutStore(first, () => 1000);
    let clockRead = false;
    const contender = createLogoutStore(second, () => {
      clockRead = true;
      return 1000;
    });
    owner.bind("old", binding);
    first.exec("BEGIN IMMEDIATE");
    const start = performance.now();
    assert.throws(() => contender.accept(logout, "a"), /locked/);
    assert.ok(performance.now() - start < 2000);
    assert.equal(clockRead, false);
    assert.deepEqual(owner.sessions(), ["old"]);
    first.exec("ROLLBACK");
    assert.equal(contender.accept(logout, "a"), "accepted");
    assert.deepEqual(owner.sessions(), []);
  } finally {
    first.close();
    second.close();
  }
});
test("a separate process commits replay state and matching termination durably", async (t) => {
  const path = await file(t);
  const database = new DatabaseSync(path);
  createLogoutStore(database, () => 1000).bind("old", binding);
  database.close();
  const module = new URL(
    "../../lib/reference-logout-store.mjs",
    import.meta.url,
  ).href;
  const script = `import { DatabaseSync } from 'node:sqlite';
    import { createLogoutStore } from ${JSON.stringify(module)};
    const db = new DatabaseSync(${JSON.stringify(path)});
    createLogoutStore(db, () => 1000).accept(${JSON.stringify(logout)}, 'same-token');
    db.close();`;
  await run(process.execPath, ["--input-type=module", "-e", script], {
    capture: true,
  });
  const reopened = new DatabaseSync(path);
  try {
    const store = createLogoutStore(reopened, () => 1000);
    assert.deepEqual(store.sessions(), []);
    assert.equal(store.accept(logout, "same-token"), "duplicate");
    assert.equal(store.bind("late", binding), false);
  } finally {
    reopened.close();
  }
});
test("independent connections serialize a racing callback with duplicate deliveries", async (t) => {
  const path = await file(t);
  const database = new DatabaseSync(path);
  const store = createLogoutStore(database, () => 1000);
  const workers = ["callback", "logout", "logout"].map(
    (action) =>
      new Worker(new URL("./reference-logout-worker.mjs", import.meta.url), {
        workerData: {
          path,
          action,
          claims: action === "logout" ? logout : binding,
        },
      }),
  );
  try {
    await Promise.all(workers.map((worker) => once(worker, "message")));
    const results = workers.map((worker) => once(worker, "message"));
    const exits = workers.map((worker) => once(worker, "exit"));
    for (const worker of workers) worker.postMessage("go");
    const values = (await Promise.all(results)).flat();
    assert.deepEqual(values.slice(1).sort(), ["accepted", "duplicate"]);
    assert.deepEqual(store.sessions(), []);
    assert.equal(store.bind("late", binding), false);
    assert.deepEqual((await Promise.all(exits)).flat(), [0, 0, 0]);
  } finally {
    await Promise.all(workers.map((worker) => worker.terminate()));
    database.close();
  }
});
