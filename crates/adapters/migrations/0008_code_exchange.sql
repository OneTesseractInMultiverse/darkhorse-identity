CREATE TABLE authorization_codes (
 digest bytea PRIMARY KEY CHECK(octet_length(digest)=32),
 request_digest bytea NOT NULL UNIQUE CHECK(octet_length(request_digest)=32),
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 client_revision bigint NOT NULL CHECK(client_revision>=0),
 application_revision bigint NOT NULL CHECK(application_revision>=0),
 session_digest bytea NOT NULL REFERENCES browser_sessions(digest),
 principal_id uuid NOT NULL REFERENCES principals(id),
 authenticated_ms bigint NOT NULL,
 redirect_uri text NOT NULL,
 challenge bytea NOT NULL CHECK(octet_length(challenge)=32),
 nonce text,
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint NOT NULL CHECK(expires_ms=created_ms+60000),
 consumed boolean NOT NULL DEFAULT false,
 FOREIGN KEY(session_digest,principal_id,authenticated_ms) REFERENCES browser_sessions(digest,principal_id,created_ms)
);
CREATE TABLE access_tokens (
 digest bytea PRIMARY KEY CHECK(octet_length(digest)=32),
 code_digest bytea NOT NULL UNIQUE REFERENCES authorization_codes(digest),
 audience text NOT NULL,
 scope text NOT NULL CHECK(scope='openid'),
 claim_ceiling text[] NOT NULL CHECK(claim_ceiling=ARRAY['sub']),
 capability_ceiling uuid[] NOT NULL CHECK(cardinality(capability_ceiling)=0),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint NOT NULL CHECK(expires_ms=created_ms+300000),
 revoked boolean NOT NULL DEFAULT false
);
CREATE INDEX access_tokens_expiry ON access_tokens(expires_ms);
CREATE TABLE token_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 principal_id uuid NOT NULL REFERENCES principals(id),
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 event text NOT NULL CHECK(event IN ('code_issued','code_redeemed','code_replayed')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0)
);
CREATE TRIGGER token_audit_immutable BEFORE UPDATE OR DELETE ON token_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION code_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'consumed') IS DISTINCT FROM (to_jsonb(OLD)-'consumed') OR OLD.consumed OR NOT NEW.consumed THEN
 RAISE EXCEPTION 'immutable authorization code' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER code_transition BEFORE UPDATE ON authorization_codes FOR EACH ROW EXECUTE FUNCTION code_transition();
CREATE FUNCTION access_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'revoked') IS DISTINCT FROM (to_jsonb(OLD)-'revoked') OR (OLD.revoked AND NOT NEW.revoked) THEN
 RAISE EXCEPTION 'immutable access grant' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER access_transition BEFORE UPDATE ON access_tokens FOR EACH ROW EXECUTE FUNCTION access_transition();
