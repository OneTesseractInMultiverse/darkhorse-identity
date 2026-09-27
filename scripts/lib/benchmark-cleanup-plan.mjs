export const cleanupSelectionPlanSql = `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT digest
FROM authorization_requests
WHERE expires_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
ORDER BY expires_ms,digest
LIMIT 100
FOR UPDATE SKIP LOCKED`;

export const cleanupDeletePlanSql = `EXPLAIN (FORMAT JSON)
WITH expired AS MATERIALIZED (
  SELECT digest
  FROM authorization_requests
  WHERE expires_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
  ORDER BY expires_ms,digest
  LIMIT 100
  FOR UPDATE SKIP LOCKED
), removed AS (
  DELETE FROM authorization_requests request
  USING expired
  WHERE request.digest=expired.digest
  RETURNING request.digest,request.expires_ms
), remaining AS (
  SELECT request.digest,request.expires_ms
  FROM authorization_requests request
  WHERE request.expires_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND NOT EXISTS (SELECT 1 FROM removed WHERE removed.digest=request.digest)
  ORDER BY request.expires_ms,request.digest
  LIMIT 1
)
SELECT (SELECT count(*)::bigint FROM removed),
  (SELECT max(floor(extract(epoch FROM clock_timestamp())*1000)::bigint-expires_ms)::bigint FROM removed),
  (SELECT (floor(extract(epoch FROM clock_timestamp())*1000)::bigint-expires_ms)::bigint FROM remaining)`;

export const cleanupTableStatisticsSql = `SELECT json_build_object(
  'relationBytes',pg_relation_size('authorization_requests'::regclass),
  'totalBytes',pg_total_relation_size('authorization_requests'::regclass),
  'liveTuplesEstimate',n_live_tup,
  'deadTuplesEstimate',n_dead_tup,
  'deletedTuplesSinceStatsReset',n_tup_del,
  'autovacuumCount',autovacuum_count,
  'lastAutovacuumAt',last_autovacuum
)::text
FROM pg_stat_user_tables
WHERE relid='authorization_requests'::regclass`;

export function summarizeCleanupQueryPlan(document) {
  if (!Array.isArray(document) || document.length !== 1)
    throw new Error("Invalid lifecycle cleanup query plan.");
  const [result] = document;
  if (!result || typeof result !== "object" || !result.Plan)
    throw new Error("Invalid lifecycle cleanup query plan.");
  let visited = 0;
  const summarize = (node, depth) => {
    visited += 1;
    if (
      !node ||
      typeof node !== "object" ||
      typeof node["Node Type"] !== "string" ||
      depth > 12 ||
      visited > 64 ||
      !Array.isArray(node.Plans ?? []) ||
      (node.Plans ?? []).length > 16
    )
      throw new Error("Invalid lifecycle cleanup query plan.");
    return {
      nodeType: node["Node Type"],
      relation:
        typeof node["Relation Name"] === "string"
          ? node["Relation Name"]
          : null,
      index: typeof node["Index Name"] === "string" ? node["Index Name"] : null,
      estimatedRows: finiteOrNull(node["Plan Rows"]),
      actualRows: finiteOrNull(node["Actual Rows"]),
      loops: finiteOrNull(node["Actual Loops"]),
      sharedBuffers: {
        hit: finiteOrNull(node["Shared Hit Blocks"]),
        read: finiteOrNull(node["Shared Read Blocks"]),
      },
      children: (node.Plans ?? []).map((child) => summarize(child, depth + 1)),
    };
  };
  return {
    planningMs: finiteOrNull(result["Planning Time"]),
    executionMs: finiteOrNull(result["Execution Time"]),
    root: summarize(result.Plan, 0),
  };
}

function finiteOrNull(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? value
    : null;
}
