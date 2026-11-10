// Test-only SQLite adapter. Inputs must already have passed cryptographic validation.
// All replicas and callback writers must use the same transactional primary store.
const schema = `
  PRAGMA busy_timeout=250;
  PRAGMA synchronous=FULL;
  CREATE TABLE IF NOT EXISTS rp_sessions (
    id TEXT PRIMARY KEY, iss TEXT NOT NULL, aud TEXT NOT NULL,
    sub TEXT NOT NULL, sid TEXT NOT NULL, expires INTEGER NOT NULL);
  CREATE INDEX IF NOT EXISTS rp_session_binding ON rp_sessions(iss,aud,sub,sid);
  CREATE INDEX IF NOT EXISTS rp_session_expiry ON rp_sessions(expires);
  CREATE TABLE IF NOT EXISTS rp_replays (
    iss TEXT NOT NULL, aud TEXT NOT NULL, jti TEXT NOT NULL,
    fingerprint TEXT NOT NULL, expires INTEGER NOT NULL, PRIMARY KEY(iss,aud,jti));
  CREATE INDEX IF NOT EXISTS rp_replay_expiry ON rp_replays(expires);
  CREATE TABLE IF NOT EXISTS rp_ended (
    iss TEXT NOT NULL, aud TEXT NOT NULL, sub TEXT NOT NULL, sid TEXT NOT NULL,
    expires INTEGER NOT NULL, PRIMARY KEY(iss,aud,sub,sid));
  CREATE INDEX IF NOT EXISTS rp_ended_expiry ON rp_ended(expires);
`;
function live(claims, now, lifetime) {
  return (
    [now, now + 3600, claims.iat, claims.exp].every(Number.isSafeInteger) &&
    claims.iat >= 0 &&
    claims.iat <= now &&
    claims.exp > now &&
    claims.exp > claims.iat &&
    claims.exp - claims.iat <= lifetime
  );
}
function tuple(claims) {
  return [claims.iss, claims.aud, claims.sub, claims.sid];
}
function transaction(db, operation) {
  db.exec("BEGIN IMMEDIATE");
  try {
    const result = operation();
    db.exec("COMMIT");
    return result;
  } catch (error) {
    db.exec("ROLLBACK");
    throw error;
  }
}
function capacity(count, limit) {
  if (count >= limit) throw new Error("Reference store full.");
}
function validLimit(limit) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 10000)
    throw new Error("Invalid reference store limit.");
}
function replayResult(prior, fingerprint) {
  return prior.fingerprint === fingerprint ? "duplicate" : "invalid";
}
function blocked(tombstone, now) {
  return tombstone !== undefined && tombstone.expires > now;
}
function deadline(now, seconds) {
  return now + seconds;
}
function sessionIds(rows) {
  return rows.map((row) => row.id);
}
function prepare(db) {
  db.exec(schema);
  const prune = ["rp_sessions", "rp_replays", "rp_ended"].map((table) =>
    db.prepare(
      `DELETE FROM ${table} WHERE rowid IN (SELECT rowid FROM ${table} WHERE expires<=? ORDER BY expires LIMIT 100)`,
    ),
  );
  const counts = ["rp_sessions", "rp_replays", "rp_ended"].map((table) =>
    db.prepare(`SELECT count(*) AS n FROM ${table}`),
  );
  const replay = db.prepare(
    "SELECT fingerprint FROM rp_replays WHERE iss=? AND aud=? AND jti=?",
  );
  const ended = db.prepare(
    "SELECT expires FROM rp_ended WHERE iss=? AND aud=? AND sub=? AND sid=?",
  );
  const insertReplay = db.prepare("INSERT INTO rp_replays VALUES(?,?,?,?,?)");
  const end = db.prepare(
    "INSERT INTO rp_ended VALUES(?,?,?,?,?) ON CONFLICT(iss,aud,sub,sid) DO UPDATE SET expires=max(expires,excluded.expires)",
  );
  const remove = db.prepare(
    "DELETE FROM rp_sessions WHERE iss=? AND aud=? AND sub=? AND sid=?",
  );
  const insert = db.prepare("INSERT INTO rp_sessions VALUES(?,?,?,?,?,?)");
  const sessions = db.prepare(
    "SELECT id FROM rp_sessions WHERE expires>? ORDER BY id",
  );
  return {
    prune,
    counts,
    replay,
    ended,
    insertReplay,
    end,
    remove,
    insert,
    sessions,
  };
}
function cleanup(store, now) {
  for (const statement of store.prune) statement.run(now);
}
function accept(store, clock, limit, claims, fingerprint) {
  const now = clock();
  if (!live(claims, now, 120)) return "invalid";
  cleanup(store, now);
  const prior = store.replay.get(claims.iss, claims.aud, claims.jti);
  if (prior) return replayResult(prior, fingerprint);
  capacity(store.counts[1].get().n, limit);
  const binding = tuple(claims);
  if (!store.ended.get(...binding)) capacity(store.counts[2].get().n, limit);
  store.insertReplay.run(
    claims.iss,
    claims.aud,
    claims.jti,
    fingerprint,
    claims.exp,
  );
  // Covers the maximum remaining ID-token lifetime even when logout precedes a callback.
  store.end.run(...binding, deadline(now, 300));
  store.remove.run(...binding);
  return "accepted";
}
function bind(store, clock, limit, id, claims) {
  const now = clock();
  if (!live(claims, now, 300)) return false;
  cleanup(store, now);
  const binding = tuple(claims);
  if (blocked(store.ended.get(...binding), now)) return false;
  capacity(store.counts[0].get().n, limit);
  store.insert.run(id, ...binding, deadline(now, 3600));
  return true;
}
export function createLogoutStore(db, clock, limit = 10000) {
  validLimit(limit);
  const store = prepare(db);
  return {
    accept: (claims, fingerprint) =>
      transaction(db, () => accept(store, clock, limit, claims, fingerprint)),
    bind: (id, claims) =>
      transaction(db, () => bind(store, clock, limit, id, claims)),
    sessions: () => sessionIds(store.sessions.all(clock())),
  };
}
