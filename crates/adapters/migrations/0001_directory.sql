-- Operator-managed schema. Runtime grants are assigned explicitly at deployment.
CREATE TABLE security_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    bootstrapped boolean NOT NULL DEFAULT false,
    policy_revision bigint NOT NULL DEFAULT 0 CHECK (policy_revision >= 0)
);
INSERT INTO security_state (singleton) VALUES (true);

CREATE TABLE principals (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    email text NOT NULL CHECK (length(email) BETWEEN 3 AND 254 AND email = btrim(email)),
    email_key text COLLATE "C" GENERATED ALWAYS AS (lower(email COLLATE "C")) STORED UNIQUE,
    first_name text NOT NULL CHECK (length(first_name) BETWEEN 1 AND 100 AND first_name = btrim(first_name) AND first_name !~ '[[:cntrl:]]'),
    last_name text NOT NULL CHECK (length(last_name) BETWEEN 1 AND 100 AND last_name = btrim(last_name) AND last_name !~ '[[:cntrl:]]'),
    active boolean NOT NULL DEFAULT true,
    credential_epoch bigint NOT NULL DEFAULT 0 CHECK (credential_epoch >= 0),
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE credentials (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    principal_id uuid NOT NULL REFERENCES principals(id),
    kind text NOT NULL CHECK (length(kind) BETWEEN 1 AND 32),
    revoked boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (id, kind)
);
CREATE INDEX credentials_principal ON credentials(principal_id);
CREATE UNIQUE INDEX one_live_password ON credentials(principal_id) WHERE kind = 'password' AND NOT revoked;
CREATE TABLE password_credentials (
    credential_id uuid PRIMARY KEY,
    kind text NOT NULL DEFAULT 'password' CHECK (kind = 'password'),
    verifier text NOT NULL CHECK (length(verifier) BETWEEN 60 AND 512 AND verifier LIKE '$argon2id$v=19$%'),
    FOREIGN KEY (credential_id, kind) REFERENCES credentials(id, kind)
);
CREATE TABLE platform_administrators (
    principal_id uuid PRIMARY KEY REFERENCES principals(id)
);
CREATE TABLE capabilities (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    permission_key text COLLATE "C" NOT NULL UNIQUE CHECK (length(permission_key) BETWEEN 1 AND 200),
    meaning text NOT NULL CHECK (length(meaning) BETWEEN 1 AND 1000),
    retired boolean NOT NULL DEFAULT false
);
CREATE TABLE security_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    database_role text NOT NULL DEFAULT session_user,
    event text NOT NULL CHECK (event IN ('bootstrap', 'account.deactivated', 'account.reactivated', 'account.revoked')),
    principal_id uuid NOT NULL REFERENCES principals(id),
    principal_revision bigint NOT NULL CHECK (principal_revision >= 0),
    credential_epoch bigint NOT NULL CHECK (credential_epoch >= 0)
);

CREATE FUNCTION forbid_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'immutable record' USING ERRCODE = '23514'; END;
$$;
CREATE TRIGGER retain_principal BEFORE DELETE ON principals FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER retain_credential BEFORE DELETE ON credentials FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER retain_capability BEFORE DELETE ON capabilities FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER retain_security_state BEFORE DELETE ON security_state FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER immutable_audit BEFORE UPDATE OR DELETE ON security_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

CREATE FUNCTION guard_principal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id <> OLD.id OR NEW.created_at <> OLD.created_at OR NEW.revision <> OLD.revision + 1
       OR NEW.credential_epoch < OLD.credential_epoch
       OR (OLD.active AND NOT NEW.active AND NEW.credential_epoch <= OLD.credential_epoch) THEN
        RAISE EXCEPTION 'invalid principal transition' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER principal_transition BEFORE UPDATE ON principals FOR EACH ROW EXECUTE FUNCTION guard_principal();

CREATE FUNCTION guard_credential() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id <> OLD.id OR NEW.principal_id <> OLD.principal_id OR NEW.kind <> OLD.kind
       OR NEW.created_at <> OLD.created_at OR (OLD.revoked AND NOT NEW.revoked) THEN
        RAISE EXCEPTION 'invalid credential transition' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER credential_transition BEFORE UPDATE ON credentials FOR EACH ROW EXECUTE FUNCTION guard_credential();

CREATE FUNCTION guard_capability() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id <> OLD.id OR NEW.permission_key <> OLD.permission_key OR NEW.meaning <> OLD.meaning OR (OLD.retired AND NOT NEW.retired) THEN
        RAISE EXCEPTION 'immutable permission identity' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER capability_identity BEFORE UPDATE ON capabilities FOR EACH ROW EXECUTE FUNCTION guard_capability();

CREATE FUNCTION guard_security_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (OLD.bootstrapped AND NOT NEW.bootstrapped) OR NEW.policy_revision < OLD.policy_revision THEN
        RAISE EXCEPTION 'invalid security state' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER security_state_transition BEFORE UPDATE ON security_state FOR EACH ROW EXECUTE FUNCTION guard_security_state();

CREATE FUNCTION bump_policy_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE security_state SET policy_revision = policy_revision + 1 WHERE singleton;
    RETURN NULL;
END;
$$;
CREATE TRIGGER principal_policy_revision AFTER INSERT OR UPDATE ON principals FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER capability_policy_revision AFTER INSERT OR UPDATE ON capabilities FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER administrator_policy_revision AFTER INSERT OR UPDATE OR DELETE ON platform_administrators FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();

CREATE FUNCTION require_active_administrator() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM singleton FROM security_state WHERE singleton FOR UPDATE;
    IF (SELECT bootstrapped FROM security_state WHERE singleton)
       AND NOT EXISTS (SELECT 1 FROM platform_administrators a JOIN principals p ON p.id = a.principal_id WHERE p.active) THEN
        RAISE EXCEPTION 'last active administrator' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER principal_administrator_guard AFTER UPDATE ON principals DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_active_administrator();
CREATE CONSTRAINT TRIGGER membership_administrator_guard AFTER UPDATE OR DELETE ON platform_administrators DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_active_administrator();
