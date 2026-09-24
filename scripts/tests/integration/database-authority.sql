\set ON_ERROR_STOP on
BEGIN;
DO $$
DECLARE statement text;
BEGIN
 IF current_user <> 'darkhorse_runtime' OR session_user <> 'darkhorse_runtime' THEN
  RAISE EXCEPTION 'test requires a real runtime login';
 END IF;
 -- Every denial must come from the privilege boundary, even with no target rows.
 -- A constraint/immutable-row trigger failure is not proof of restricted grants.
 FOREACH statement IN ARRAY ARRAY[
  'SELECT * FROM future_operator_state',
  'UPDATE signing_operation_intents SET operation_id=operation_id WHERE false',
  'DELETE FROM signing_operation_intents WHERE false',
  'TRUNCATE signing_operation_intents',
  'SELECT * FROM signing_operation_intents',
  'INSERT INTO signing_operation_intents(operation_id) SELECT operation_id FROM signing_operation_intents WHERE false',
  'UPDATE signing_operation_receipts SET operation_id=operation_id WHERE false',
  'DELETE FROM signing_operation_receipts WHERE false',
  'TRUNCATE signing_operation_receipts',
  'SELECT * FROM signing_operation_receipts',
  'INSERT INTO signing_operation_receipts(operation_id) SELECT operation_id FROM signing_operation_receipts WHERE false',
  'UPDATE limiter_activation_intents SET operation_id=operation_id WHERE false',
  'DELETE FROM limiter_activation_intents WHERE false',
  'TRUNCATE limiter_activation_intents',
  'SELECT * FROM limiter_activation_intents',
  'INSERT INTO limiter_activation_intents(operation_id) SELECT operation_id FROM limiter_activation_intents WHERE false',
  'UPDATE limiter_activation_receipts SET operation_id=operation_id WHERE false',
  'DELETE FROM limiter_activation_receipts WHERE false',
  'TRUNCATE limiter_activation_receipts',
  'SELECT * FROM limiter_activation_receipts',
  'INSERT INTO limiter_activation_receipts(operation_id) SELECT operation_id FROM limiter_activation_receipts WHERE false',
  'UPDATE future_operator_state SET secret=''forged'' WHERE false',
  'INSERT INTO future_operator_state VALUES (''forged'')',
  'SELECT nextval(''future_operator_sequence'')',
  'SELECT future_operator_function()',
  'SELECT * FROM _sqlx_migrations',
  'CREATE TABLE runtime_owned(value text)',
  'SET ROLE darkhorse_owner',
  'SET ROLE darkhorse_operator',
  'INSERT INTO platform_administrators SELECT principal_id FROM platform_administrators WHERE false',
  'DELETE FROM platform_administrators WHERE false',
  'UPDATE signing_keys SET phase=phase WHERE false',
  'UPDATE limiter_authority SET active=active WHERE false',
  'INSERT INTO provider_audit(event,kid,revision,occurred_ms) SELECT event,kid,revision,occurred_ms FROM provider_audit WHERE false',
  'INSERT INTO limiter_audit(epoch,generation,event) SELECT epoch,generation,event FROM limiter_audit WHERE false',
  'UPDATE provider_state SET revision=revision WHERE false',
  'UPDATE security_state SET bootstrapped=bootstrapped WHERE false',
  'UPDATE login_budget_policy SET key_digest=key_digest WHERE false',
  'UPDATE email_delivery_state SET origin=origin WHERE false',
  'UPDATE media_configuration SET storage_fingerprint=storage_fingerprint WHERE false',
  'TRUNCATE security_audit',
  'TRUNCATE operator_directory_audit',
  'TRUNCATE operator_catalog_audit',
  'TRUNCATE operator_catalog_detail_audit',
  'TRUNCATE operator_application_audit',
  'TRUNCATE operator_client_audit',
  'ALTER TABLE security_audit DISABLE TRIGGER ALL',
  'DROP TABLE security_audit',
  'SELECT setval(''security_audit_id_seq'', 1)'
 ] LOOP
  BEGIN
   EXECUTE statement;
   RAISE EXCEPTION 'unexpected runtime authority: %', statement;
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
   RAISE EXCEPTION 'unexpected audit mutation privilege: %', statement;
  EXCEPTION WHEN insufficient_privilege THEN NULL;
  END;
 END LOOP;
END $$;

-- CHECK constraints invoke only these explicitly granted pure validators.
DO $$ BEGIN
 IF NOT oidc_identity_scopes(ARRAY['openid'])
  OR oidc_identity_scopes(ARRAY['forbidden'])
  OR oidc_identity_claims(ARRAY['openid']) <> ARRAY['sub']
  OR NOT capability_ceiling_valid(ARRAY['00000000-0000-0000-0000-000000000001'::uuid])
  OR NOT resource_token_scopes(ARRAY['openid','read']) THEN
  RAISE EXCEPTION 'constraint validator grants or behavior failed';
 END IF;
END $$;

-- Existing lock-only coordination still works with column-specific UPDATE.
SELECT singleton FROM security_state FOR UPDATE;
SELECT singleton FROM authorization_capacity FOR UPDATE;
UPDATE security_state SET policy_revision=policy_revision+1 WHERE singleton;
INSERT INTO principals(id,email,first_name,last_name)
 VALUES('00000000-0000-0000-0000-000000000123','authority@example.com','Authority','Fixture');
INSERT INTO security_audit(event,principal_id,principal_revision,credential_epoch)
 VALUES('account.revoked','00000000-0000-0000-0000-000000000123',0,0);
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM security_audit WHERE database_role='darkhorse_runtime'
   AND principal_id='00000000-0000-0000-0000-000000000123') THEN
  RAISE EXCEPTION 'runtime audit insert or role provenance failed';
 END IF;
END $$;
ROLLBACK;
