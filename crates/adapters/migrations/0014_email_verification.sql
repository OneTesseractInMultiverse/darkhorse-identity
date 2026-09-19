-- Current-email ownership is independent from permission and credential state.
ALTER TABLE principals ADD COLUMN email_verified_ms bigint CHECK (email_verified_ms >= 0);
CREATE FUNCTION reset_email_proof() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.email IS DISTINCT FROM OLD.email THEN NEW.email_verified_ms = NULL; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER reset_email_proof BEFORE UPDATE ON principals FOR EACH ROW EXECUTE FUNCTION reset_email_proof();

CREATE TABLE email_delivery_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    origin text NOT NULL,
    key_fingerprint bytea NOT NULL CHECK (octet_length(key_fingerprint)=32)
);
CREATE TRIGGER immutable_email_delivery_state BEFORE UPDATE OR DELETE ON email_delivery_state FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

-- One bounded current proof per principal; seed alone cannot reproduce the HMAC token.
CREATE TABLE email_verifications (
    principal_id uuid PRIMARY KEY REFERENCES principals(id),
    id uuid NOT NULL UNIQUE CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    actor_session_id uuid NOT NULL,
    email text NOT NULL,
    credential_epoch bigint NOT NULL CHECK (credential_epoch >= 0),
    digest bytea NOT NULL UNIQUE CHECK (octet_length(digest)=32),
    seed bytea CHECK (octet_length(seed)=32),
    created_ms bigint NOT NULL CHECK (created_ms >= 0),
    expires_ms bigint NOT NULL CHECK (expires_ms=created_ms+900000),
    consumed boolean NOT NULL DEFAULT false,
    attempts smallint NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    next_ms bigint NOT NULL CHECK (next_ms >= created_ms),
    delivery_state text NOT NULL DEFAULT 'queued' CHECK (delivery_state IN ('queued','accepted','failed','cancelled')),
    CHECK ((delivery_state='queued') = (seed IS NOT NULL)),
    FOREIGN KEY (actor_session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id)
);
CREATE INDEX email_delivery_due ON email_verifications(next_ms) WHERE delivery_state='queued';
CREATE TABLE email_verification_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    principal_id uuid NOT NULL REFERENCES principals(id),
    verification_id uuid NOT NULL,
    actor_session_id uuid,
    event text NOT NULL CHECK (event IN ('requested','verified','accepted','retry','failed')),
    attempt smallint NOT NULL DEFAULT 0 CHECK (attempt BETWEEN 0 AND 5),
    occurred_ms bigint NOT NULL CHECK (occurred_ms >= 0),
    database_role text NOT NULL DEFAULT session_user,
    FOREIGN KEY (actor_session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id)
);
CREATE INDEX email_request_budget ON email_verification_audit(principal_id,occurred_ms) WHERE event='requested';
CREATE TRIGGER immutable_email_verification_audit BEFORE UPDATE OR DELETE ON email_verification_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

CREATE FUNCTION cancel_changed_email_proof() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.email IS DISTINCT FROM OLD.email THEN
        UPDATE email_verifications SET consumed=true,seed=NULL,delivery_state='cancelled' WHERE principal_id=NEW.id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER cancel_changed_email_proof AFTER UPDATE ON principals FOR EACH ROW EXECUTE FUNCTION cancel_changed_email_proof();

CREATE FUNCTION guard_email_verification() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.principal_id <> OLD.principal_id THEN
        RAISE EXCEPTION 'immutable proof owner' USING ERRCODE='23514';
    END IF;
    IF NEW.id = OLD.id THEN
        IF (NEW.actor_session_id,NEW.email,NEW.credential_epoch,NEW.digest,NEW.created_ms,NEW.expires_ms)
           IS DISTINCT FROM (OLD.actor_session_id,OLD.email,OLD.credential_epoch,OLD.digest,OLD.created_ms,OLD.expires_ms)
           OR (OLD.consumed AND NOT NEW.consumed)
           OR NEW.attempts < OLD.attempts OR NEW.attempts > OLD.attempts + 1
           OR (NEW.seed IS NOT NULL AND NEW.seed IS DISTINCT FROM OLD.seed)
           OR (OLD.delivery_state <> 'queued' AND NEW.delivery_state = 'queued') THEN
            RAISE EXCEPTION 'invalid proof transition' USING ERRCODE='23514';
        END IF;
    ELSIF NEW.created_ms < OLD.created_ms + 900000 OR NEW.consumed OR NEW.attempts <> 0 OR NEW.delivery_state <> 'queued' THEN
        RAISE EXCEPTION 'invalid proof replacement' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER email_verification_transition BEFORE UPDATE ON email_verifications FOR EACH ROW EXECUTE FUNCTION guard_email_verification();
CREATE TRIGGER retain_email_verification BEFORE DELETE ON email_verifications FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
