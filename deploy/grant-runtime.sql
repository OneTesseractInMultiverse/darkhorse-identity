-- Dedicated application database only. Stop serving and reapply after reviewed
-- migrations, as the schema owner or cluster administrator. Never grant runtime
-- membership in other roles. See docs/database-authority.md for the trust boundary.
\set ON_ERROR_STOP on
BEGIN;
SET LOCAL search_path = pg_catalog, public;
DO $$
DECLARE runtime_oid oid;
BEGIN
 SELECT oid INTO runtime_oid FROM pg_roles WHERE rolname='darkhorse_runtime'
  AND NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole
  AND NOT rolreplication AND NOT rolbypassrls;
 IF runtime_oid IS NULL
  OR EXISTS(SELECT 1 FROM pg_auth_members WHERE member=runtime_oid)
  OR EXISTS(SELECT 1 FROM pg_database WHERE oid=(SELECT oid FROM pg_database WHERE datname=current_database()) AND datdba=runtime_oid)
  OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspowner=runtime_oid)
  OR EXISTS(SELECT 1 FROM pg_class WHERE relowner=runtime_oid)
  OR EXISTS(SELECT 1 FROM pg_proc WHERE proowner=runtime_oid)
 THEN
  RAISE EXCEPTION 'runtime must be a nonowner role without elevated attributes or memberships';
 END IF;
 EXECUTE format('REVOKE ALL ON DATABASE %I FROM PUBLIC, darkhorse_runtime',current_database());
 EXECUTE format('GRANT CONNECT ON DATABASE %I TO darkhorse_runtime',current_database());
END $$;

-- Rebuild grants, including prior column grants, inside one transaction. RESTRICT
-- intentionally fails on dependent grants rather than silently cascading them.
REVOKE ALL ON SCHEMA public FROM PUBLIC, darkhorse_runtime;
REVOKE ALL ON ALL TABLES IN SCHEMA public FROM PUBLIC, darkhorse_runtime;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA public FROM PUBLIC, darkhorse_runtime;
REVOKE ALL ON ALL ROUTINES IN SCHEMA public FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON TABLES FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON SEQUENCES FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner REVOKE ALL ON ROUTINES FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON TABLES FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON SEQUENCES FROM PUBLIC, darkhorse_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE darkhorse_owner IN SCHEMA public REVOKE ALL ON ROUTINES FROM PUBLIC, darkhorse_runtime;
GRANT USAGE ON SCHEMA public TO darkhorse_runtime;
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
COMMIT;
