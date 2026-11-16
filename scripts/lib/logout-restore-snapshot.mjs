// Test-only, secret-free projections. A stopped source and quarantined restore
// must match exactly; this does not authorize resuming restored workers.
import { isDeepStrictEqual } from "node:util";

const shapes = {
  logout_account_intents:
    "id:n principal_id:u credential_epoch:z created_ms:z cursor:u? expanded:b",
  credential_logout_events:
    "ordering:n credential_id:u principal_id:u credential_epoch:z created_ms:z database_role:t",
  logout_credential_intents: "id:n created_ms:z cursor:u? expanded:b",
  logout_intents:
    "id:n session_id:u created_ms:z cursor:u? expanded:b destination_order:n?",
  logout_deliveries:
    "id:u intent_id:n sid:u client_id:u revision:n state:d attempts:a created_ms:z next_ms:z",
  logout_destinations: "client_id:u revision:n ordering:n uri:h?",
  logout_audit:
    "id:n client_id:u? actor_id:u? delivery_id:u? revision:n? event:e occurred_ms:z database_role:t",
  browser_sessions:
    "public_id:u principal_id:u credential_id:u credential_epoch:z",
  credentials: "id:u principal_id:u kind:t revoked:b",
  relying_party_sessions:
    "sid:u issuer:h session_id:u principal_id:u client_id:u created_ms:z database_role:t",
  sequences: "name:q last_value:n is_called:b",
};
const fields = Object.fromEntries(
  Object.entries(shapes).map(([name, shape]) => [
    name,
    shape.split(" ").map((field) => field.split(":")),
  ]),
);
const tables = Object.keys(shapes).slice(0, 7);
const maxBytes = 2 * 1024 * 1024;
const maxRows = 1024;
const uuid =
  /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const queues = {
  logout_account_intents: ["cursor", "expanded"],
  logout_credential_intents: ["cursor", "expanded"],
  logout_intents: ["cursor", "expanded"],
  logout_deliveries: ["state", "attempts", "next_ms"],
};
function requireFact(ok, category) {
  if (!ok) throw new Error(`Logout restore ${category} rejected.`);
}
function boundedText(text) {
  requireFact(
    typeof text === "string" && Buffer.byteLength(text) <= maxBytes,
    "size",
  );
}
function valid(value, type) {
  if (type.endsWith("?"))
    return value === null || valid(value, type.slice(0, -1));
  if (typeof value !== "string") return false;
  if (["n", "z", "a"].includes(type)) {
    if (!/^(0|[1-9][0-9]{0,18})$/.test(value)) return false;
    const number = BigInt(value);
    return (
      number <= (type === "a" ? 8n : 9223372036854775807n) &&
      (type !== "n" || number > 0n)
    );
  }
  if (type === "u") return uuid.test(value);
  if (type === "b") return value === "true" || value === "false";
  if (type === "d")
    return ["queued", "delivered", "failed", "cancelled"].includes(value);
  if (type === "e")
    return [
      "configured",
      "claimed",
      "delivered",
      "retry",
      "failed",
      "cancelled",
    ].includes(value);
  if (type === "q")
    return ["logout_order", "logout_audit_id_seq"].includes(value);
  if (type === "h")
    return (
      value.length >= 9 &&
      Buffer.byteLength(value) <= 2048 &&
      /^https:\/\/[^\s]+$/.test(value)
    );
  return (
    value.length > 0 && value.length <= 64 && !/[\x00-\x1f\x7f]/.test(value)
  );
}
function key(table, row) {
  return table === "logout_destinations"
    ? `${row.client_id}/${row.revision}`
    : row[fields[table][0][0]];
}
function index(table, rows) {
  const result = new Map(rows.map((row) => [key(table, row), row]));
  requireFact(result.size === rows.length, "duplicate row");
  return result;
}
export function parseLogoutSnapshot(text) {
  boundedText(text);
  let entries;
  try {
    entries = JSON.parse(text);
  } catch {
    requireFact(false, "JSON");
  }
  requireFact(
    Array.isArray(entries) && entries.length === Object.keys(fields).length,
    "categories",
  );
  const result = {};
  for (const entry of entries) {
    requireFact(Array.isArray(entry) && entry.length === 2, "category shape");
    const [table, rows] = entry;
    requireFact(
      typeof table === "string" &&
        Object.hasOwn(fields, table) &&
        !Object.hasOwn(result, table),
      "categories",
    );
    requireFact(
      Array.isArray(rows) && rows.length > 0 && rows.length <= maxRows,
      "row count",
    );
    result[table] = rows.map((row) => {
      requireFact(
        Array.isArray(row) && row.length === fields[table].length,
        "row shape",
      );
      requireFact(
        row.every((value, i) => valid(value, fields[table][i][1])),
        "field",
      );
      return Object.fromEntries(
        fields[table].map(([name], i) => [name, row[i]]),
      );
    });
    index(table, result[table]);
  }
  relationships(result);
  return result;
}
function relationships(s) {
  const maps = Object.fromEntries(
    Object.entries(s).map(([table, rows]) => [table, index(table, rows)]),
  );
  const session = (id) => maps.browser_sessions.get(id);
  const matches = (row, owner, epoch, credential) =>
    row &&
    row.principal_id === owner &&
    row.credential_epoch === epoch &&
    (!credential || row.credential_id === credential);
  for (const event of s.credential_logout_events) {
    const credential = maps.credentials.get(event.credential_id);
    requireFact(
      credential?.principal_id === event.principal_id &&
        credential.kind === "password" &&
        credential.revoked === "true",
      "credential event binding",
    );
  }
  for (const root of s.logout_credential_intents) {
    const event = maps.credential_logout_events.get(root.id);
    requireFact(
      event?.created_ms === root.created_ms,
      "credential root binding",
    );
    requireFact(
      root.cursor === null ||
        matches(
          session(root.cursor),
          event.principal_id,
          event.credential_epoch,
          event.credential_id,
        ),
      "credential cursor",
    );
  }
  for (const root of s.logout_account_intents)
    requireFact(
      root.cursor === null ||
        matches(session(root.cursor), root.principal_id, root.credential_epoch),
      "account cursor",
    );
  for (const rp of s.relying_party_sessions)
    requireFact(
      session(rp.session_id)?.principal_id === rp.principal_id,
      "RP ownership",
    );
  const origins = new Set();
  for (const intent of s.logout_intents) {
    const owner = session(intent.session_id);
    requireFact(owner, "session binding");
    const sources = [
      ...s.logout_account_intents
        .filter((r) => matches(owner, r.principal_id, r.credential_epoch))
        .map((r) => ({ order: r.id, time: r.created_ms, kind: "account" })),
      ...s.credential_logout_events
        .filter((r) =>
          matches(owner, r.principal_id, r.credential_epoch, r.credential_id),
        )
        .map((r) => ({
          order: r.ordering,
          time: r.created_ms,
          kind: "credential",
        })),
    ]
      .filter((r) => BigInt(r.order) < BigInt(intent.id))
      .sort((a, b) => (BigInt(a.order) < BigInt(b.order) ? -1 : 1));
    if (sources.length) {
      requireFact(
        intent.destination_order === sources[0].order &&
          intent.created_ms === sources[0].time,
        "copied source order/time",
      );
      origins.add(sources[0].kind);
    } else requireFact(intent.destination_order === null, "missing source");
    requireFact(
      intent.cursor === null ||
        s.relying_party_sessions.some(
          (rp) =>
            rp.session_id === intent.session_id &&
            rp.client_id === intent.cursor,
        ),
      "session cursor",
    );
  }
  for (const delivery of s.logout_deliveries) {
    const intent = maps.logout_intents.get(delivery.intent_id);
    const rp = maps.relying_party_sessions.get(delivery.sid);
    requireFact(
      intent &&
        rp?.session_id === intent.session_id &&
        rp.client_id === delivery.client_id,
      "delivery RP binding",
    );
    const destinations = s.logout_destinations
      .filter(
        (d) =>
          d.client_id === delivery.client_id &&
          BigInt(d.ordering) < BigInt(intent.destination_order ?? intent.id),
      )
      .sort((a, b) => (BigInt(a.ordering) > BigInt(b.ordering) ? -1 : 1));
    requireFact(
      destinations[0]?.revision === delivery.revision &&
        destinations[0].uri !== null,
      "original destination",
    );
    requireFact(
      delivery.created_ms === intent.created_ms &&
        BigInt(delivery.next_ms) >= BigInt(delivery.created_ms),
      "delivery time",
    );
  }
  for (const audit of s.logout_audit) {
    const delivery = maps.logout_deliveries.get(audit.delivery_id);
    // Terminal payloads may be absent while immutable audit remains retained.
    if (delivery)
      requireFact(
        audit.client_id === delivery.client_id &&
          audit.revision === delivery.revision &&
          BigInt(audit.occurred_ms) >= BigInt(delivery.created_ms),
        "audit binding",
      );
  }
  const orders = [
    ...s.logout_account_intents.map((r) => r.id),
    ...s.credential_logout_events.map((r) => r.ordering),
    ...s.logout_intents.map((r) => r.id),
    ...s.logout_destinations.map((r) => r.ordering),
  ];
  requireFact(
    new Set(orders).size === orders.length,
    "source order uniqueness",
  );
  requireFact(s.sequences.length === 2, "sequences");
  for (const [name, values] of [
    ["logout_order", orders],
    ["logout_audit_id_seq", s.logout_audit.map((r) => r.id)],
  ]) {
    const sequence = maps.sequences.get(name);
    requireFact(
      sequence?.is_called === "true" &&
        values.every((value) => BigInt(value) <= BigInt(sequence.last_value)),
      "sequence position",
    );
  }
  requireFact(
    origins.has("account") && origins.has("credential"),
    "representative sources",
  );
  for (const table of [
    "logout_account_intents",
    "logout_credential_intents",
    "logout_intents",
  ])
    requireFact(
      s[table].some((row) => row.expanded === "true" && row.cursor !== null) &&
        s[table].some((row) => row.expanded === "false"),
      "representative cursors",
    );
  requireFact(
    s.logout_deliveries.some(
      (d) =>
        d.state === "queued" &&
        BigInt(d.attempts) > 0n &&
        BigInt(d.next_ms) > BigInt(d.created_ms),
    ) && s.logout_deliveries.some((d) => d.state !== "queued"),
    "representative progress",
  );
  requireFact(
    s.logout_deliveries.some((d) =>
      s.logout_destinations.some(
        (v) =>
          v.client_id === d.client_id &&
          BigInt(v.revision) > BigInt(d.revision),
      ),
    ),
    "destination history",
  );
  requireFact(
    ["configured", "claimed", "retry", "delivered"].every((event) =>
      s.logout_audit.some((a) => a.event === event),
    ),
    "representative history",
  );
}
function projection(table, rows, names) {
  return rows
    .map((row) => [key(table, row), ...names.map((name) => row[name])])
    .sort((a, b) => a[0].localeCompare(b[0], "en"));
}
export function assertLogoutRestore(source, restored) {
  for (const table of Object.keys(fields)) {
    const names = fields[table].map(([name]) => name);
    const progress = queues[table];
    if (!progress) {
      requireFact(
        isDeepStrictEqual(
          projection(table, source[table], names),
          projection(table, restored[table], names),
        ),
        "immutable history or binding equality",
      );
      continue;
    }
    requireFact(
      isDeepStrictEqual(
        projection(
          table,
          source[table],
          names.filter((name) => !progress.includes(name)),
        ),
        projection(
          table,
          restored[table],
          names.filter((name) => !progress.includes(name)),
        ),
      ),
      "queue binding equality",
    );
    // These columns are mutable in service, but no advancement is allowed in
    // this stopped-source/quarantined-restore checkpoint.
    requireFact(
      isDeepStrictEqual(
        projection(table, source[table], progress),
        projection(table, restored[table], progress),
      ),
      "quarantined queue progress equality",
    );
  }
}
export function assertLogoutArchive(text) {
  boundedText(text);
  const required = new Set(
    tables
      .flatMap((table) => [`TABLE/${table}`, `TABLE DATA/${table}`])
      .concat([
        "SEQUENCE SET/logout_order",
        "SEQUENCE SET/logout_audit_id_seq",
      ]),
  );
  const found = new Set();
  for (const line of text.split("\n")) {
    const match =
      /^\d+;\s+\d+\s+\d+\s+(TABLE DATA|TABLE|SEQUENCE SET) public ([a-z_]+) \S+\s*$/.exec(
        line,
      );
    const item = match && `${match[1]}/${match[2]}`;
    if (!required.has(item)) continue;
    requireFact(!found.has(item), "duplicate archive entry");
    found.add(item);
  }
  requireFact(found.size === required.size, "archive inventory");
}

// Every numeric column is exported as text, so bigint order/time never round.
// The extra row makes overflow an explicit failure instead of a partial pass.
export const logoutSnapshotQuery = `BEGIN READ ONLY;
SET LOCAL statement_timeout='5s';
SET LOCAL lock_timeout='1s';
SELECT jsonb_build_array(${Object.entries(fields)
  .filter(([name]) => name !== "sequences")
  .map(
    ([table, columns]) =>
      `jsonb_build_array('${table}',(SELECT COALESCE(jsonb_agg(row), '[]'::jsonb) FROM (SELECT jsonb_build_array(${columns.map(([name]) => `${name}::text`).join(",")}) AS row FROM ${table} LIMIT ${maxRows + 1}) bounded))`,
  )
  .join(",\n")},
jsonb_build_array('sequences',jsonb_build_array(
 (SELECT jsonb_build_array('logout_order',last_value::text,is_called::text) FROM logout_order),
 (SELECT jsonb_build_array('logout_audit_id_seq',last_value::text,is_called::text) FROM logout_audit_id_seq))));
COMMIT;`;
