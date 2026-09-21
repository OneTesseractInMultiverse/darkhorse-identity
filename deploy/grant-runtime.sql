-- Reapply only after a reviewed migration. This separates ownership/DDL and
-- selected operator operations; runtime DML remains a trusted application boundary.
\set ON_ERROR_STOP on
BEGIN;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO darkhorse_runtime;
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO darkhorse_runtime;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO darkhorse_runtime;
REVOKE ALL ON _sqlx_migrations FROM darkhorse_runtime;
REVOKE INSERT,UPDATE,DELETE ON platform_administrators,signing_keys,limiter_authority,limiter_audit FROM darkhorse_runtime;
REVOKE UPDATE ON provider_state FROM darkhorse_runtime;
GRANT UPDATE(last_ms) ON provider_state TO darkhorse_runtime;
COMMIT;
