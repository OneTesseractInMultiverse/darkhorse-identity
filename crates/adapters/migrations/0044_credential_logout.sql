-- Retain the original credential transition separately from disposable queue state.
-- This preserves its destination snapshot even after queue retention has run.
CREATE TABLE credential_logout_events (
 ordering bigint PRIMARY KEY DEFAULT nextval('logout_order'),
 credential_id uuid NOT NULL UNIQUE REFERENCES credentials(id),
 principal_id uuid NOT NULL REFERENCES principals(id),
 credential_epoch bigint NOT NULL CHECK(credential_epoch >= 0),
 created_ms bigint NOT NULL CHECK(created_ms >= 0),
 database_role text NOT NULL DEFAULT session_user,
 UNIQUE(ordering,created_ms)
);
CREATE TRIGGER credential_logout_event_immutable BEFORE UPDATE OR DELETE ON credential_logout_events
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

CREATE TABLE logout_credential_intents (
 id bigint PRIMARY KEY,
 created_ms bigint NOT NULL,
 cursor uuid,
 expanded boolean NOT NULL DEFAULT false,
 FOREIGN KEY(id,created_ms) REFERENCES credential_logout_events(ordering,created_ms)
);
CREATE INDEX logout_credential_intents_pending ON logout_credential_intents(id) WHERE NOT expanded;
CREATE INDEX logout_credential_intents_retention ON logout_credential_intents(created_ms,id) WHERE expanded;
CREATE INDEX browser_sessions_logout_credential ON browser_sessions(credential_id,credential_epoch,principal_id,public_id);

-- Take the common authority fence before touching any credential rows, including
-- SQL maintenance writers. Existing application writers already hold this fence.
CREATE FUNCTION lock_credential_logout() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM singleton FROM security_state WHERE singleton FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'missing authority fence' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER credential_logout_fence BEFORE UPDATE OF revoked ON credentials
 FOR EACH STATEMENT EXECUTE FUNCTION lock_credential_logout();

-- Capture one immutable event and one bounded-work root in the revocation's
-- transaction. This records a fact; it does not authorize an administrative action.
CREATE FUNCTION record_password_logout() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_id bigint; event_time bigint; changed bigint;
BEGIN
 event_time := floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
 INSERT INTO credential_logout_events(credential_id,principal_id,credential_epoch,created_ms)
 SELECT NEW.id,NEW.principal_id,p.credential_epoch,event_time FROM principals p WHERE p.id=NEW.principal_id
 RETURNING ordering INTO event_id;
 GET DIAGNOSTICS changed = ROW_COUNT;
 IF changed <> 1 THEN RAISE EXCEPTION 'missing credential logout event' USING ERRCODE='23514'; END IF;
 INSERT INTO logout_credential_intents(id,created_ms) VALUES(event_id,event_time);
 GET DIAGNOSTICS changed = ROW_COUNT;
 IF changed <> 1 THEN RAISE EXCEPTION 'missing credential logout intent' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER password_logout AFTER UPDATE OF revoked ON credentials
 FOR EACH ROW WHEN (OLD.kind='password' AND NOT OLD.revoked AND NEW.revoked)
 EXECUTE FUNCTION record_password_logout();
