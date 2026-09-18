-- Capacity is serialized independently; security_state is always locked first.
CREATE TABLE authorization_capacity(singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton));
INSERT INTO authorization_capacity DEFAULT VALUES;
CREATE TRIGGER authorization_capacity_retained BEFORE UPDATE OR DELETE ON authorization_capacity FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
ALTER TABLE browser_sessions ADD CONSTRAINT browser_session_binding UNIQUE(digest,principal_id,created_ms);
CREATE TABLE authorization_requests (
 digest bytea PRIMARY KEY CHECK(octet_length(digest)=32),
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 client_revision bigint NOT NULL CHECK(client_revision>=0),
 application_revision bigint NOT NULL CHECK(application_revision>=0),
 redirect_uri text NOT NULL CHECK(octet_length(redirect_uri) BETWEEN 1 AND 2048),
 challenge bytea NOT NULL CHECK(octet_length(challenge)=32),
 state text CHECK(octet_length(state) BETWEEN 1 AND 512),
 nonce text CHECK(octet_length(nonce) BETWEEN 1 AND 256),
 scopes text[] NOT NULL CHECK(cardinality(scopes) BETWEEN 1 AND 32),
 resource text CHECK(octet_length(resource) BETWEEN 1 AND 256),
 prompt text NOT NULL CHECK(prompt IN ('default','none','login','consent','login_consent')),
 max_age bigint CHECK(max_age BETWEEN 0 AND 28800),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint NOT NULL CHECK(expires_ms=created_ms+300000),
 initial_session bytea CHECK(octet_length(initial_session)=32),
 bound_session bytea REFERENCES browser_sessions(digest),
 principal_id uuid REFERENCES principals(id),
 authenticated_ms bigint,
 approved boolean NOT NULL DEFAULT false,
 terminal boolean NOT NULL DEFAULT false,
 FOREIGN KEY(bound_session,principal_id,authenticated_ms) REFERENCES browser_sessions(digest,principal_id,created_ms),
 CHECK((bound_session IS NULL)=(principal_id IS NULL)),
 CHECK((bound_session IS NULL)=(authenticated_ms IS NULL)),
 CHECK(authenticated_ms IS NULL OR authenticated_ms>=0),
 CHECK(NOT approved OR bound_session IS NOT NULL)
);
CREATE INDEX authorization_expiry ON authorization_requests(expires_ms);
CREATE INDEX authorization_client ON authorization_requests(client_id);
CREATE FUNCTION authorization_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-ARRAY['bound_session','principal_id','authenticated_ms','approved','terminal']) IS DISTINCT FROM
    (to_jsonb(OLD)-ARRAY['bound_session','principal_id','authenticated_ms','approved','terminal'])
    OR OLD.terminal OR (OLD.approved AND NOT NEW.approved)
    OR (OLD.bound_session IS NOT NULL AND (NEW.bound_session IS DISTINCT FROM OLD.bound_session OR NEW.principal_id IS DISTINCT FROM OLD.principal_id OR NEW.authenticated_ms IS DISTINCT FROM OLD.authenticated_ms))
 THEN RAISE EXCEPTION 'immutable authorization request' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER authorization_transition BEFORE UPDATE ON authorization_requests FOR EACH ROW EXECUTE FUNCTION authorization_transition();
-- Consent records are bounded by principal/client/resource, not by arbitrary scope combinations.
CREATE TABLE oauth_consents (
 principal_id uuid NOT NULL REFERENCES principals(id),
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 resource text NOT NULL,
 client_revision bigint NOT NULL,
 application_revision bigint NOT NULL,
 scopes text[] NOT NULL CHECK(cardinality(scopes) BETWEEN 1 AND 32),
 PRIMARY KEY(principal_id,client_id,resource)
);

CREATE TABLE consent_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 principal_id uuid NOT NULL REFERENCES principals(id),
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 resource text NOT NULL,
 scopes text[] NOT NULL,
 occurred_ms bigint NOT NULL
);
CREATE TRIGGER consent_audit_immutable BEFORE UPDATE OR DELETE ON consent_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
