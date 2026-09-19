-- Public references are independent of the secret session verifier.
ALTER TABLE browser_sessions ADD COLUMN public_id uuid NOT NULL DEFAULT gen_random_uuid()
 CHECK(public_id<>'00000000-0000-0000-0000-000000000000');
ALTER TABLE browser_sessions ADD UNIQUE(public_id);
ALTER TABLE browser_sessions ADD UNIQUE(public_id,principal_id);
CREATE INDEX browser_sessions_directory ON browser_sessions(principal_id,created_ms DESC,public_id DESC);
CREATE FUNCTION browser_session_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-ARRAY['seen_ms','revoked']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['seen_ms','revoked'])
    OR NEW.seen_ms<OLD.seen_ms OR (OLD.revoked AND NOT NEW.revoked) THEN
  RAISE EXCEPTION 'immutable or terminal session state' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER browser_session_transition BEFORE UPDATE ON browser_sessions
 FOR EACH ROW EXECUTE FUNCTION browser_session_transition();
CREATE TABLE session_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 principal_id uuid NOT NULL REFERENCES principals(id),
 session_id uuid NOT NULL,
 actor_session_id uuid,
 event text NOT NULL CHECK(event IN ('created','replaced','signed_out','session_ended')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 FOREIGN KEY(session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id),
 FOREIGN KEY(actor_session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id),
 CHECK((event='created')=(actor_session_id IS NULL))
);
CREATE INDEX session_audit_principal ON session_audit(principal_id,id);
CREATE TRIGGER session_audit_immutable BEFORE UPDATE OR DELETE ON session_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
