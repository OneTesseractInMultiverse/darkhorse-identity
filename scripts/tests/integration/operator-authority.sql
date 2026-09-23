\set ON_ERROR_STOP on
BEGIN;
DO $$
DECLARE statement text;
BEGIN
 IF current_user <> 'darkhorse_operator' OR session_user <> 'darkhorse_operator' THEN
  RAISE EXCEPTION 'test requires a real operator login';
 END IF;
 FOREACH statement IN ARRAY ARRAY[
  'SELECT * FROM future_operator_state',
  'UPDATE limiter_activation_intents SET operation_id=operation_id WHERE false',
  'DELETE FROM limiter_activation_intents WHERE false',
  'TRUNCATE limiter_activation_intents',
  'UPDATE limiter_activation_receipts SET operation_id=operation_id WHERE false',
  'DELETE FROM limiter_activation_receipts WHERE false',
  'TRUNCATE limiter_activation_receipts',
  'INSERT INTO limiter_activation_intents(database_role) SELECT database_role FROM limiter_activation_intents WHERE false',
  'INSERT INTO limiter_activation_intents(prepared_ms) SELECT prepared_ms FROM limiter_activation_intents WHERE false',
  'INSERT INTO limiter_activation_receipts(completed_ms) SELECT completed_ms FROM limiter_activation_receipts WHERE false',
  'SELECT nextval(''future_operator_sequence'')',
  'SELECT future_operator_function()',
  'SELECT * FROM _sqlx_migrations',
  'CREATE TABLE operator_owned(value text)',
  'CREATE TEMP TABLE operator_temporary(value text)',
  'CREATE SCHEMA operator_owned',
  'SET ROLE darkhorse_owner',
  'SET ROLE darkhorse_runtime',
  'CREATE ROLE operator_escalated',
  'UPDATE password_credentials SET verifier=verifier WHERE false',
  'UPDATE credentials SET revoked=revoked WHERE false',
  'UPDATE principals SET email=email WHERE false',
  'DELETE FROM platform_administrators WHERE false',
  'UPDATE provider_state SET wrap_digest=wrap_digest WHERE false',
  'UPDATE login_budget_policy SET key_digest=key_digest WHERE false',
  'SELECT * FROM browser_sessions',
  'SELECT * FROM access_tokens',
  'SELECT * FROM oauth_client_secrets',
  'SELECT * FROM personal_key_verifiers',
  'UPDATE invitations SET digest=digest WHERE false',
  'TRUNCATE provider_audit',
  'ALTER TABLE provider_audit DISABLE TRIGGER ALL',
  'DROP TABLE provider_audit',
  'SELECT setval(''security_audit_id_seq'', 1)'
 ] LOOP
  BEGIN
   EXECUTE statement;
   RAISE EXCEPTION 'unexpected operator authority: %', statement;
  EXCEPTION WHEN insufficient_privilege THEN NULL;
  END;
 END LOOP;
 FOR statement IN
  SELECT format('%s %I %s', operation, c.relname, suffix)
  FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
  CROSS JOIN (VALUES ('UPDATE','SET id=DEFAULT WHERE false'),('DELETE FROM','WHERE false')) AS probes(operation,suffix)
  WHERE n.nspname='public' AND c.relkind='r' AND c.relname LIKE '%\_audit' ESCAPE '\'
 LOOP
  BEGIN
   EXECUTE statement;
   RAISE EXCEPTION 'unexpected operator audit mutation: %', statement;
  EXCEPTION WHEN insufficient_privilege THEN NULL;
  END;
 END LOOP;
END $$;
SELECT singleton FROM security_state FOR UPDATE;
SELECT credential_id FROM password_credentials FOR SHARE;
SELECT id FROM credentials FOR SHARE;
ROLLBACK;
