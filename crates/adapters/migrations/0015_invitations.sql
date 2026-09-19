-- Ordinary-account onboarding. Immutable proof identity; no implicit access grants.
CREATE TABLE invitations (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    email text NOT NULL CHECK (length(email) BETWEEN 3 AND 254 AND email=btrim(email)),
    email_key text COLLATE "C" GENERATED ALWAYS AS (lower(email COLLATE "C")) STORED,
    issuer_id uuid NOT NULL REFERENCES principals(id),
    issuer_credential_id uuid NOT NULL REFERENCES credentials(id),
    issuer_epoch bigint NOT NULL CHECK (issuer_epoch>=0),
    digest bytea NOT NULL UNIQUE CHECK (octet_length(digest)=32),
    seed bytea CHECK (octet_length(seed)=32),
    created_ms bigint NOT NULL CHECK (created_ms>=0),
    expires_ms bigint NOT NULL CHECK (expires_ms=created_ms+86400000),
    closed boolean NOT NULL DEFAULT false,
    principal_id uuid REFERENCES principals(id),
    hash_attempts smallint NOT NULL DEFAULT 0 CHECK (hash_attempts BETWEEN 0 AND 5),
    attempts smallint NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    next_ms bigint NOT NULL CHECK (next_ms>=created_ms),
    delivery_state text NOT NULL DEFAULT 'queued' CHECK (delivery_state IN ('queued','accepted','failed','cancelled')),
    CHECK ((delivery_state='queued')=(seed IS NOT NULL)),
    CHECK (NOT closed OR seed IS NULL),
    CHECK (principal_id IS NULL OR closed)
);
CREATE UNIQUE INDEX one_open_invitation ON invitations(email_key) WHERE NOT closed;
CREATE INDEX invitation_recent ON invitations(created_ms DESC,id DESC);
CREATE INDEX invitation_recipient_budget ON invitations(email_key,created_ms);
CREATE INDEX invitation_issuer_budget ON invitations(issuer_id,created_ms);
CREATE INDEX invitation_delivery_due ON invitations(next_ms) WHERE delivery_state='queued';
CREATE TABLE invitation_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    invitation_id uuid NOT NULL REFERENCES invitations(id),
    actor_id uuid REFERENCES principals(id),
    event text NOT NULL CHECK (event IN ('invited','revoked','hash_admitted','created','accepted','retry','failed')),
    attempt smallint NOT NULL DEFAULT 0 CHECK (attempt BETWEEN 0 AND 5),
    occurred_ms bigint NOT NULL CHECK (occurred_ms>=0),
    database_role text NOT NULL DEFAULT session_user
);
CREATE INDEX invitation_hash_budget ON invitation_audit(occurred_ms) WHERE event='hash_admitted';
CREATE TRIGGER immutable_invitation_audit BEFORE UPDATE OR DELETE ON invitation_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER retain_invitation BEFORE DELETE ON invitations FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_invitation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (NEW.id,NEW.email,NEW.issuer_id,NEW.issuer_credential_id,NEW.issuer_epoch,NEW.digest,NEW.created_ms,NEW.expires_ms)
       IS DISTINCT FROM (OLD.id,OLD.email,OLD.issuer_id,OLD.issuer_credential_id,OLD.issuer_epoch,OLD.digest,OLD.created_ms,OLD.expires_ms)
       OR (OLD.closed AND NOT NEW.closed)
       OR (OLD.closed AND NEW.principal_id IS DISTINCT FROM OLD.principal_id)
       OR (OLD.principal_id IS NOT NULL AND NEW.principal_id IS DISTINCT FROM OLD.principal_id)
       OR NEW.hash_attempts < OLD.hash_attempts OR NEW.hash_attempts > OLD.hash_attempts+1
       OR NEW.attempts < OLD.attempts OR NEW.attempts > OLD.attempts+1
       OR (NEW.seed IS NOT NULL AND NEW.seed IS DISTINCT FROM OLD.seed)
       OR (OLD.delivery_state <> 'queued' AND NEW.delivery_state='queued') THEN
       RAISE EXCEPTION 'invalid invitation transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER invitation_transition BEFORE UPDATE ON invitations FOR EACH ROW EXECUTE FUNCTION guard_invitation();
-- Demotion is terminal for pending invitations even if membership is later restored.
CREATE FUNCTION cancel_demoted_invitations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE issuer_id=OLD.principal_id AND NOT closed;
    RETURN OLD;
END;
$$;
CREATE TRIGGER cancel_demoted_invitations AFTER DELETE ON platform_administrators FOR EACH ROW EXECUTE FUNCTION cancel_demoted_invitations();
CREATE FUNCTION cancel_ineligible_invitations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT NEW.active OR NEW.credential_epoch <> OLD.credential_epoch THEN
        UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE issuer_id=NEW.id AND NOT closed;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER cancel_ineligible_invitations AFTER UPDATE ON principals FOR EACH ROW EXECUTE FUNCTION cancel_ineligible_invitations();

CREATE FUNCTION cancel_revoked_issuer_invitations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.revoked THEN
        UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE issuer_credential_id=NEW.id AND NOT closed;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER cancel_revoked_issuer_invitations AFTER UPDATE ON credentials FOR EACH ROW EXECUTE FUNCTION cancel_revoked_issuer_invitations();
