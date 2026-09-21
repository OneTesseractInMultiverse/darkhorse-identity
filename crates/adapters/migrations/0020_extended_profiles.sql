-- Unicode character limits require a UTF-8 database, including existing deployments.
DO $$ BEGIN
 IF current_setting('server_encoding')<>'UTF8' THEN RAISE EXCEPTION 'Darkhorse profiles require a UTF8 database; migrate the database encoding before applying this migration.'; END IF;
END $$;
-- Descriptive attributes share the principal revision, never credential authority.
ALTER TABLE principals
 ADD COLUMN second_name text CHECK(second_name IS NULL OR length(second_name) BETWEEN 1 AND 100),
 ADD COLUMN second_last_name text CHECK(second_last_name IS NULL OR length(second_last_name) BETWEEN 1 AND 100),
 ADD COLUMN country text CHECK(country IS NULL OR country ~ '^[A-Z]{2}$'),
 ADD COLUMN calling_code text CHECK(calling_code IS NULL OR calling_code ~ '^[1-9][0-9]{0,2}$'),
 ADD COLUMN national_number text CHECK(national_number IS NULL OR national_number ~ '^[0-9]+$'),
 ADD COLUMN bio text CHECK(bio IS NULL OR (length(bio) BETWEEN 1 AND 2000 AND octet_length(bio)<=8000)),
 ADD CONSTRAINT profile_phone_pair CHECK((calling_code IS NULL)=(national_number IS NULL)),
 ADD CONSTRAINT profile_phone_length CHECK(length(calling_code||national_number)<=15);
CREATE TABLE profile_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES principals(id),
 actor_session_id uuid NOT NULL,
 target_id uuid NOT NULL REFERENCES principals(id),
 target_revision bigint NOT NULL CHECK(target_revision>=0),
 event text NOT NULL CHECK(event IN ('profile_updated')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_session_id,actor_id) REFERENCES browser_sessions(public_id,principal_id)
);
CREATE INDEX profile_audit_target ON profile_audit(target_id,id);
CREATE TRIGGER profile_audit_immutable BEFORE UPDATE OR DELETE ON profile_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
