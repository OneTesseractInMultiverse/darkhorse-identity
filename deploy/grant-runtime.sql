-- Dedicated application database only. Stop serving and reapply after reviewed
-- migrations, as the schema owner or cluster administrator. Neither nonowner
-- role may have memberships. See docs/database-authority.md for the trust boundary.
\set ON_ERROR_STOP on
BEGIN;
SET LOCAL search_path = pg_catalog, public;
DO $$
DECLARE role_oid oid; role_name text;
BEGIN
 FOREACH role_name IN ARRAY ARRAY['darkhorse_runtime','darkhorse_operator'] LOOP
  SELECT oid INTO role_oid FROM pg_roles WHERE rolname=role_name
   AND NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole
   AND NOT rolreplication AND NOT rolbypassrls;
  IF role_oid IS NULL
   OR EXISTS(SELECT 1 FROM pg_auth_members WHERE member=role_oid)
   OR EXISTS(SELECT 1 FROM pg_database WHERE oid=(SELECT oid FROM pg_database WHERE datname=current_database()) AND datdba=role_oid)
   OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspowner=role_oid)
   OR EXISTS(SELECT 1 FROM pg_class WHERE relowner=role_oid)
   OR EXISTS(SELECT 1 FROM pg_proc WHERE proowner=role_oid)
  THEN
   RAISE EXCEPTION '% must be a nonowner role without elevated attributes or memberships', replace(role_name,'darkhorse_','');
  END IF;
 END LOOP;
 EXECUTE format('REVOKE ALL ON DATABASE %I FROM PUBLIC, darkhorse_runtime, darkhorse_operator',current_database());
 EXECUTE format('GRANT CONNECT ON DATABASE %I TO darkhorse_runtime, darkhorse_operator',current_database());
END $$;

-- Rebuild grants, including prior column grants, inside one transaction. RESTRICT
-- intentionally fails on dependent grants rather than silently cascading them.
REVOKE ALL ON SCHEMA public FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
REVOKE ALL ON ALL TABLES IN SCHEMA public FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA public FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
REVOKE ALL ON ALL ROUTINES IN SCHEMA public FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON TABLES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON SEQUENCES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON ROUTINES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON TABLES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON SEQUENCES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON ROUTINES FROM PUBLIC, darkhorse_runtime, darkhorse_operator;
GRANT USAGE ON SCHEMA public TO darkhorse_runtime, darkhorse_operator;
-- These four pure, invoker-rights validators are used by CHECK constraints.
GRANT EXECUTE ON FUNCTION
 oidc_identity_scopes(text[]),oidc_identity_claims(text[]),
 capability_ceiling_valid(uuid[]),resource_token_scopes(text[])
 TO darkhorse_runtime;

-- Existing application workflows retain DML on these named relations. This is
-- not row-level authorization or containment of a compromised application.
GRANT SELECT,INSERT,UPDATE,DELETE ON
 principals,credentials,password_credentials,capabilities,browser_sessions,
 applications,protected_resources,resource_scopes,oauth_clients,client_redirects,
 client_resources,client_scopes,oauth_client_secrets,authorization_requests,
 oauth_consents,authorization_codes,access_tokens,capability_applications,roles,
 role_applications,role_capabilities,resource_capabilities,scope_capabilities,
 principal_roles,authorization_resource_grants,resource_introspection,
 resource_introspection_secrets,refresh_families,refresh_tokens,invitations,
 email_verifications,relying_party_sessions,personal_keys,personal_key_verifiers,
 personal_key_grants,media_assets,branding_settings
 TO darkhorse_runtime;

-- Audit retention: append/read only. Identity columns generate their own values;
-- callers need no direct sequence privileges. No audit-edit grants.
GRANT SELECT,INSERT ON
 security_audit,registration_audit,consent_audit,token_audit,
 authorization_policy_audit,resource_registration_audit,session_audit,
 email_verification_audit,invitation_audit,directory_admin_audit,
 catalog_admin_audit,personal_key_audit,profile_audit,media_audit,operator_account_audit
 TO darkhorse_runtime;
GRANT SELECT ON
 platform_administrators,eligible_administrators,signing_keys,limiter_authority,
 limiter_audit,provider_audit,personal_key_policy
 TO darkhorse_runtime;

-- Runtime startup validates immutable deployment bindings (INSERT ON CONFLICT).
GRANT SELECT,INSERT ON
 provider_state,login_budget_policy,email_delivery_state,media_configuration
 TO darkhorse_runtime;
GRANT UPDATE(last_ms) ON provider_state TO darkhorse_runtime;
GRANT SELECT ON security_state,authorization_capacity TO darkhorse_runtime;
GRANT UPDATE(policy_revision) ON security_state TO darkhorse_runtime;
-- PostgreSQL row locking requires UPDATE on at least one column.
GRANT UPDATE(singleton) ON authorization_capacity TO darkhorse_runtime;
-- Infrastructure operations and authenticated account CLI only. No inherited
-- runtime privileges, token/session/client tables, migration history or DDL.
GRANT SELECT,INSERT ON principals,credentials,password_credentials,platform_administrators
 TO darkhorse_operator;
GRANT SELECT ON eligible_administrators,security_state TO darkhorse_operator;
GRANT UPDATE(active,credential_epoch,revision) ON principals TO darkhorse_operator;
GRANT UPDATE(bootstrapped,policy_revision) ON security_state TO darkhorse_operator;
-- Credential rechecks use FOR SHARE. Fixed kind columns permit locking without
-- granting credential revocation, verifier replacement or identity changes.
GRANT UPDATE(kind) ON credentials,password_credentials TO darkhorse_operator;
-- The existing principal-change trigger cancels this account's invitations.
GRANT SELECT(issuer_id,closed),UPDATE(closed,seed,delivery_state) ON invitations
 TO darkhorse_operator;
GRANT SELECT,INSERT ON login_budget_policy,provider_state,signing_keys,
 limiter_authority,security_audit,operator_account_audit,provider_audit,limiter_audit
 TO darkhorse_operator;
GRANT UPDATE(revision,last_ms) ON provider_state TO darkhorse_operator;
GRANT UPDATE(phase,activated_ms,verify_until_ms,ciphertext) ON signing_keys TO darkhorse_operator;
GRANT UPDATE(epoch,generation,active,not_before_ms,run_id,replication_id) ON limiter_authority
 TO darkhorse_operator;
GRANT SELECT ON limiter_activation_intents,limiter_activation_receipts TO darkhorse_operator;
GRANT INSERT(operation_id,epoch,generation,not_before_ms) ON limiter_activation_intents TO darkhorse_operator;
GRANT INSERT(operation_id,run_id,replication_id) ON limiter_activation_receipts TO darkhorse_operator;
GRANT SELECT ON signing_operation_intents,signing_operation_receipts TO darkhorse_operator;
GRANT INSERT(operation_id,operation,issuer,kid,expected_revision) ON signing_operation_intents TO darkhorse_operator;
GRANT INSERT(operation_id,audit_id) ON signing_operation_receipts TO darkhorse_operator;
COMMIT;
