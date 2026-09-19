// Only fixed categories and counters leave PostgreSQL; never return query text.
export const profileSql = `WITH statements AS (
  SELECT *, CASE
    WHEN query ~* '^BEGIN' THEN 'begin'
    WHEN query ~* '^COMMIT' THEN 'commit'
    WHEN query ~* '^ROLLBACK' THEN 'rollback'
    WHEN query LIKE '%FROM security_state%' THEN 'security_fence'
    WHEN query LIKE 'SELECT floor(extract(epoch FROM clock_timestamp()%' THEN 'clock'
    WHEN query ~* '^(INSERT|UPDATE|DELETE)' THEN 'mutation'
    WHEN query LIKE '%FROM resource_introspection %' THEN 'resource_authentication'
    WHEN query LIKE '%FROM resource_capabilities %' OR query LIKE '%FROM principal_roles %'
      OR query LIKE '%FROM resource_scopes %' OR query LIKE '%FROM capabilities %'
      OR query LIKE '%FROM protected_resources %' THEN 'policy_projection'
    WHEN query LIKE '%FROM access_tokens %' OR query LIKE '%FROM authorization_codes %' THEN 'token_read'
    WHEN query LIKE '%FROM browser_sessions %' OR query LIKE '%FROM provider_state %' OR query LIKE '%FROM principals %'
      OR query LIKE '%FROM oauth_clients %' OR query LIKE '%FROM oauth_consents %' THEN 'identity_read'
    ELSE 'other'
  END AS category
  FROM pg_stat_statements
  WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
    AND toplevel AND query IS NOT NULL AND query NOT LIKE '%pg_stat_%'
), groups AS (
  SELECT category, sum(calls) AS calls, sum(total_exec_time) AS execution_ms,
    sum(rows) AS rows, sum(shared_blks_hit) AS shared_hits, sum(shared_blks_read) AS shared_reads,
    sum(shared_blks_dirtied) AS shared_dirtied, sum(shared_blks_written) AS shared_written,
    sum(wal_records) AS wal_records, sum(wal_fpi) AS wal_full_page_images, sum(wal_bytes)::text AS wal_bytes
  FROM statements GROUP BY category
)
SELECT json_build_object(
  'groups', COALESCE((SELECT json_agg(groups ORDER BY execution_ms DESC) FROM groups), '[]'),
  'deallocations', (SELECT dealloc::text FROM pg_stat_statements_info),
  'wal_lsn', pg_current_wal_insert_lsn()::text);`;
export const quietSql = `SELECT COALESCE(bool_and(state='idle' AND xact_start IS NULL), true)
FROM pg_stat_activity WHERE datname=current_database() AND application_name='darkhorse';`;
export const resetSql = `DO $$ BEGIN PERFORM pg_stat_statements_reset(0, (SELECT oid FROM pg_database WHERE datname=current_database()), 0); END $$;`;
