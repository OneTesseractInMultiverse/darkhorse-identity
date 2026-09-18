-- Each machine credential represents exactly one existing resource audience.
CREATE TABLE resource_introspection (
 resource_id uuid PRIMARY KEY,
 application_id uuid NOT NULL,
 active boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0),
 FOREIGN KEY(application_id,resource_id) REFERENCES protected_resources(application_id,id)
);
CREATE FUNCTION resource_introspection_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.resource_id<>OLD.resource_id OR NEW.application_id<>OLD.application_id OR NEW.revision<>OLD.revision+1 THEN
  RAISE EXCEPTION 'invalid resource registration transition' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER resource_introspection_transition BEFORE UPDATE ON resource_introspection FOR EACH ROW EXECUTE FUNCTION resource_introspection_transition();
CREATE TRIGGER resource_introspection_retained BEFORE DELETE ON resource_introspection FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TABLE resource_introspection_secrets (
 id uuid PRIMARY KEY CHECK(id<>'00000000-0000-0000-0000-000000000000'),
 resource_id uuid NOT NULL REFERENCES resource_introspection(resource_id),
 verifier bytea NOT NULL UNIQUE CHECK(octet_length(verifier)=32),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 expires_ms bigint CHECK(expires_ms>=created_ms),
 retired boolean NOT NULL DEFAULT false
);
CREATE UNIQUE INDEX resource_current_secret ON resource_introspection_secrets(resource_id) WHERE expires_ms IS NULL AND NOT retired;
CREATE UNIQUE INDEX resource_overlapping_secret ON resource_introspection_secrets(resource_id) WHERE expires_ms IS NOT NULL AND NOT retired;
CREATE FUNCTION resource_secret_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.id<>OLD.id OR NEW.resource_id<>OLD.resource_id OR NEW.verifier<>OLD.verifier OR NEW.created_ms<>OLD.created_ms
   OR (OLD.retired AND NOT NEW.retired)
   OR (OLD.expires_ms IS NOT NULL AND NEW.expires_ms IS DISTINCT FROM OLD.expires_ms)
   OR (NEW.expires_ms IS NOT NULL AND NEW.expires_ms>floor(extract(epoch FROM clock_timestamp())*1000)::bigint+300000) THEN
  RAISE EXCEPTION 'invalid resource secret transition' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER resource_secret_transition BEFORE UPDATE ON resource_introspection_secrets FOR EACH ROW EXECUTE FUNCTION resource_secret_transition();
CREATE TRIGGER resource_secrets_retained BEFORE DELETE ON resource_introspection_secrets FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER resource_introspection_policy BEFORE INSERT OR UPDATE ON resource_introspection FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER resource_secrets_policy BEFORE INSERT OR UPDATE ON resource_introspection_secrets FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TABLE resource_registration_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES principals(id),
 resource_id uuid NOT NULL REFERENCES resource_introspection(resource_id),
 revision bigint NOT NULL CHECK(revision>=0),
 event text NOT NULL CHECK(event IN ('registered','rotated','enabled','disabled')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0)
);
CREATE TRIGGER resource_audit_retained BEFORE UPDATE OR DELETE ON resource_registration_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
-- Public internal references, never authentication credentials. Existing rows are backfilled.
ALTER TABLE access_tokens ADD COLUMN credential_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE CHECK(credential_id<>'00000000-0000-0000-0000-000000000000');
