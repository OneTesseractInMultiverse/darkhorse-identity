-- Session handles are never stored: digest is SHA-256 over 256 random bits.
CREATE TABLE browser_sessions (
    digest bytea PRIMARY KEY CHECK (octet_length(digest) = 32),
    principal_id uuid NOT NULL REFERENCES principals(id),
    credential_id uuid NOT NULL REFERENCES credentials(id),
    credential_epoch bigint NOT NULL CHECK (credential_epoch >= 0),
    created_ms bigint NOT NULL CHECK (created_ms >= 0),
    seen_ms bigint NOT NULL CHECK (seen_ms >= created_ms),
    expires_ms bigint NOT NULL CHECK (expires_ms >= created_ms AND expires_ms <= created_ms + 28800000),
    revoked boolean NOT NULL DEFAULT false
);
CREATE INDEX browser_sessions_principal ON browser_sessions(principal_id);
CREATE INDEX browser_sessions_expiry ON browser_sessions(expires_ms);

-- A deployment cannot silently split account budgets by starting replicas with
-- different keys. Key replacement requires a future explicit recovery workflow.
CREATE TABLE login_budget_policy (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    key_digest bytea NOT NULL CHECK (octet_length(key_digest) = 32)
);
CREATE TRIGGER immutable_login_budget_policy BEFORE UPDATE OR DELETE ON login_budget_policy
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
