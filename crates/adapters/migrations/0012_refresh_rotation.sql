ALTER TABLE oauth_clients ADD COLUMN refresh_tokens boolean NOT NULL DEFAULT false;
ALTER TABLE provider_state ADD CONSTRAINT provider_issuer_unique UNIQUE(issuer);

CREATE TABLE refresh_families (
 code_digest bytea PRIMARY KEY REFERENCES authorization_codes(digest),
 id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid() CHECK(id<>'00000000-0000-0000-0000-000000000000'),
 issuer text COLLATE "C" NOT NULL REFERENCES provider_state(issuer),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint NOT NULL CHECK(expires_ms>created_ms AND expires_ms-created_ms<=28800000),
 revoked boolean NOT NULL DEFAULT false
);
CREATE INDEX refresh_family_expiry ON refresh_families(expires_ms,code_digest);
CREATE TABLE refresh_tokens (
 digest bytea PRIMARY KEY CHECK(octet_length(digest)=32),
 code_digest bytea NOT NULL REFERENCES refresh_families(code_digest) ON DELETE CASCADE,
 generation smallint NOT NULL CHECK(generation BETWEEN 0 AND 255),
 scope text NOT NULL,
 capability_ceiling uuid[] NOT NULL,
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint NOT NULL CHECK(expires_ms>created_ms AND expires_ms-created_ms<=900000),
 consumed boolean NOT NULL DEFAULT false,
 UNIQUE(code_digest,generation)
);
CREATE UNIQUE INDEX one_live_refresh ON refresh_tokens(code_digest) WHERE NOT consumed;
CREATE FUNCTION refresh_family_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'revoked') IS DISTINCT FROM (to_jsonb(OLD)-'revoked')
    OR (OLD.revoked AND NOT NEW.revoked) THEN
  RAISE EXCEPTION 'immutable refresh family' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER refresh_family_transition BEFORE UPDATE ON refresh_families FOR EACH ROW EXECUTE FUNCTION refresh_family_transition();
CREATE TRIGGER refresh_member_transition BEFORE UPDATE ON refresh_tokens FOR EACH ROW EXECUTE FUNCTION code_transition();
CREATE FUNCTION refresh_family_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM authorization_codes c JOIN oauth_clients o ON o.id=c.client_id
   WHERE c.digest=NEW.code_digest AND c.consumed AND o.active AND o.refresh_tokens
   AND NEW.created_ms>=c.authenticated_ms AND NEW.expires_ms<=c.authenticated_ms+28800000) THEN
  RAISE EXCEPTION 'invalid refresh family binding' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER refresh_family_binding BEFORE INSERT ON refresh_families FOR EACH ROW EXECUTE FUNCTION refresh_family_binding();
CREATE FUNCTION refresh_member_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.consumed THEN RAISE EXCEPTION 'new refresh member already consumed' USING ERRCODE='23514'; END IF;
 IF NOT EXISTS(SELECT 1 FROM refresh_families f JOIN authorization_codes c ON c.digest=f.code_digest
   WHERE f.code_digest=NEW.code_digest AND NOT f.revoked
   AND NEW.created_ms>=f.created_ms AND NEW.expires_ms<=f.expires_ms
   AND string_to_array(NEW.scope,' ') <@ c.scopes AND NEW.capability_ceiling <@ c.capability_ceiling
   AND CASE WHEN c.resource_id IS NULL
     THEN oidc_identity_scopes(string_to_array(NEW.scope,' ')) AND cardinality(NEW.capability_ceiling)=0
     ELSE resource_token_scopes(string_to_array(NEW.scope,' ')) AND capability_ceiling_valid(NEW.capability_ceiling) END) THEN
  RAISE EXCEPTION 'invalid refresh member binding' USING ERRCODE='23514';
 END IF;
 IF NEW.generation>0 AND NOT EXISTS(SELECT 1 FROM refresh_tokens previous
   WHERE previous.code_digest=NEW.code_digest AND previous.generation=NEW.generation-1
   AND previous.consumed AND NEW.created_ms>=previous.created_ms
   AND string_to_array(NEW.scope,' ') <@ string_to_array(previous.scope,' ')
   AND NEW.capability_ceiling <@ previous.capability_ceiling) THEN
  RAISE EXCEPTION 'refresh grant expanded or skipped generation' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER refresh_member_binding BEFORE INSERT ON refresh_tokens FOR EACH ROW EXECUTE FUNCTION refresh_member_binding();

ALTER TABLE access_tokens DROP CONSTRAINT access_tokens_code_digest_key;
ALTER TABLE access_tokens ADD COLUMN refresh_generation smallint;
ALTER TABLE access_tokens ADD FOREIGN KEY(code_digest,refresh_generation) REFERENCES refresh_tokens(code_digest,generation) ON DELETE CASCADE;
-- Keep an unfiltered code-digest prefix for family revocation and FK lookups.
-- NULLS NOT DISTINCT also permits only one legacy access token per code.
CREATE UNIQUE INDEX one_access_per_generation ON access_tokens(code_digest,refresh_generation) NULLS NOT DISTINCT;
ALTER TABLE access_tokens DROP CONSTRAINT access_tokens_check;
ALTER TABLE access_tokens ADD CONSTRAINT access_lifetime CHECK(expires_ms>created_ms AND expires_ms-created_ms<=300000);
CREATE OR REPLACE FUNCTION validate_access_grant() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM authorization_codes c WHERE c.digest=NEW.code_digest
   AND c.consumed AND c.resource_id IS NOT DISTINCT FROM NEW.resource_id
   AND string_to_array(NEW.scope,' ') <@ c.scopes AND NEW.capability_ceiling <@ c.capability_ceiling) THEN
  RAISE EXCEPTION 'access grant exceeds authorization code' USING ERRCODE='23514';
 END IF;
 IF NEW.refresh_generation IS NULL THEN
  IF EXISTS(SELECT 1 FROM refresh_families WHERE code_digest=NEW.code_digest) THEN
   RAISE EXCEPTION 'refresh family requires a member binding' USING ERRCODE='23514';
  END IF;
 ELSIF NOT EXISTS(SELECT 1 FROM refresh_tokens r JOIN refresh_families f USING(code_digest)
   WHERE r.code_digest=NEW.code_digest AND r.generation=NEW.refresh_generation
   AND NOT r.consumed AND NOT f.revoked AND NEW.created_ms=r.created_ms
   AND NEW.expires_ms<=r.expires_ms AND NEW.scope=r.scope
   AND NEW.capability_ceiling=r.capability_ceiling) THEN
  RAISE EXCEPTION 'access grant exceeds refresh member' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;

ALTER TABLE token_audit ADD COLUMN refresh_family_id uuid;
ALTER TABLE token_audit ADD COLUMN refresh_generation smallint CHECK(refresh_generation BETWEEN 0 AND 255);
ALTER TABLE token_audit DROP CONSTRAINT token_audit_event_check;
ALTER TABLE token_audit ADD CONSTRAINT token_audit_event_check CHECK(event IN
 ('code_issued','code_redeemed','code_replayed','access_revoked','refresh_issued','refresh_rotated','refresh_replayed','refresh_revoked'));
ALTER TABLE token_audit ADD CONSTRAINT refresh_audit_context CHECK(
 (event LIKE 'refresh_%' AND refresh_family_id IS NOT NULL AND refresh_generation IS NOT NULL)
 OR (event NOT LIKE 'refresh_%' AND refresh_family_id IS NULL AND refresh_generation IS NULL));
CREATE FUNCTION refresh_family_retention() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF floor(extract(epoch FROM clock_timestamp())*1000)::bigint-OLD.expires_ms<86400000 THEN
  RAISE EXCEPTION 'refresh family retention not reached' USING ERRCODE='23514';
 END IF;
 RETURN OLD;
END $$;
CREATE TRIGGER refresh_family_retention BEFORE DELETE ON refresh_families FOR EACH ROW EXECUTE FUNCTION refresh_family_retention();
CREATE FUNCTION refresh_member_retention() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM refresh_families WHERE code_digest=OLD.code_digest) THEN
  RAISE EXCEPTION 'retain refresh history until family cleanup' USING ERRCODE='23514';
 END IF;
 RETURN OLD;
END $$;
CREATE TRIGGER refresh_member_retention BEFORE DELETE ON refresh_tokens FOR EACH ROW EXECUTE FUNCTION refresh_member_retention();
