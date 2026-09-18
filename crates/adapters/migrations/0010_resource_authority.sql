CREATE TABLE capability_applications (
 application_id uuid NOT NULL REFERENCES applications(id),
 capability_id uuid NOT NULL REFERENCES capabilities(id),
 PRIMARY KEY(application_id,capability_id)
);
CREATE TABLE roles (
 id uuid PRIMARY KEY CHECK(id<>'00000000-0000-0000-0000-000000000000'),
 name text NOT NULL CHECK(char_length(name) BETWEEN 1 AND 100)
);
CREATE TRIGGER roles_immutable BEFORE UPDATE OR DELETE ON roles FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TABLE role_applications (
 application_id uuid NOT NULL REFERENCES applications(id),
 role_id uuid NOT NULL REFERENCES roles(id),
 PRIMARY KEY(application_id,role_id)
);
CREATE TABLE role_capabilities (
 role_id uuid NOT NULL REFERENCES roles(id),
 capability_id uuid NOT NULL REFERENCES capabilities(id),
 PRIMARY KEY(role_id,capability_id)
);
CREATE INDEX role_bindings ON role_applications(role_id,application_id);
CREATE INDEX capability_roles ON role_capabilities(capability_id,role_id);
CREATE TABLE resource_capabilities (
 application_id uuid NOT NULL,
 resource_id uuid NOT NULL,
 capability_id uuid NOT NULL,
 PRIMARY KEY(application_id,resource_id,capability_id),
 FOREIGN KEY(application_id,resource_id) REFERENCES protected_resources(application_id,id),
 FOREIGN KEY(application_id,capability_id) REFERENCES capability_applications(application_id,capability_id)
);
CREATE TABLE scope_capabilities (
 application_id uuid NOT NULL,
 resource_id uuid NOT NULL,
 scope_id uuid NOT NULL,
 capability_id uuid NOT NULL,
 PRIMARY KEY(scope_id,capability_id),
 FOREIGN KEY(application_id,resource_id,scope_id) REFERENCES resource_scopes(application_id,resource_id,id),
 FOREIGN KEY(application_id,resource_id,capability_id) REFERENCES resource_capabilities(application_id,resource_id,capability_id)
);
CREATE TABLE principal_roles (
 principal_id uuid NOT NULL REFERENCES principals(id),
 application_id uuid NOT NULL,
 role_id uuid NOT NULL,
 PRIMARY KEY(principal_id,application_id,role_id),
 FOREIGN KEY(application_id,role_id) REFERENCES role_applications(application_id,role_id)
);
CREATE INDEX role_assignments ON principal_roles(application_id,role_id);
CREATE FUNCTION validate_role_bindings() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE invalid boolean := false;
BEGIN
 -- Inspect only bindings affected by this row, including the final state of a deferred write.
 IF TG_TABLE_NAME='role_applications' AND TG_OP<>'DELETE' THEN
  SELECT EXISTS(SELECT 1 FROM role_applications a JOIN role_capabilities r ON r.role_id=a.role_id
   WHERE a.role_id=NEW.role_id AND a.application_id=NEW.application_id
   AND NOT EXISTS(SELECT 1 FROM capability_applications c WHERE c.application_id=a.application_id AND c.capability_id=r.capability_id)) INTO invalid;
 ELSIF TG_TABLE_NAME='role_capabilities' AND TG_OP<>'DELETE' THEN
  SELECT EXISTS(SELECT 1 FROM role_applications a JOIN role_capabilities r ON r.role_id=a.role_id
   WHERE r.role_id=NEW.role_id AND r.capability_id=NEW.capability_id
   AND NOT EXISTS(SELECT 1 FROM capability_applications c WHERE c.application_id=a.application_id AND c.capability_id=r.capability_id)) INTO invalid;
 ELSIF TG_TABLE_NAME='capability_applications' AND TG_OP<>'INSERT' THEN
  SELECT EXISTS(SELECT 1 FROM role_applications a JOIN role_capabilities r ON r.role_id=a.role_id
   WHERE a.application_id=OLD.application_id AND r.capability_id=OLD.capability_id
   AND NOT EXISTS(SELECT 1 FROM capability_applications c WHERE c.application_id=a.application_id AND c.capability_id=r.capability_id)) INTO invalid;
 END IF;
 IF invalid THEN
  RAISE EXCEPTION 'role exceeds explicit capability bindings' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER role_binding_guard AFTER INSERT OR UPDATE OR DELETE ON role_applications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_role_bindings();
CREATE CONSTRAINT TRIGGER role_capability_guard AFTER INSERT OR UPDATE OR DELETE ON role_capabilities DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_role_bindings();
CREATE CONSTRAINT TRIGGER capability_binding_guard AFTER INSERT OR UPDATE OR DELETE ON capability_applications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_role_bindings();

CREATE TABLE authorization_policy_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 database_role text NOT NULL DEFAULT session_user,
 policy_revision bigint NOT NULL,
 relation_name text NOT NULL,
 operation text NOT NULL,
 previous_value jsonb,
 current_value jsonb
);
CREATE TRIGGER policy_audit_immutable BEFORE UPDATE OR DELETE ON authorization_policy_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION audit_authorization_policy() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO authorization_policy_audit(policy_revision,relation_name,operation,previous_value,current_value)
 SELECT policy_revision,TG_TABLE_NAME,TG_OP,to_jsonb(OLD),to_jsonb(NEW) FROM security_state WHERE singleton;
 RETURN NULL;
END $$;
-- Writers acquire the common authority fence before changing graph rows.
DO $$ DECLARE relation text; BEGIN
 FOREACH relation IN ARRAY ARRAY['capability_applications','roles','role_applications','role_capabilities','resource_capabilities','scope_capabilities','principal_roles'] LOOP
  EXECUTE format('CREATE TRIGGER graph_revision BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision()',relation);
  EXECUTE format('CREATE TRIGGER graph_audit AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION audit_authorization_policy()',relation);
 END LOOP;
END $$;

CREATE FUNCTION capability_ceiling_valid(caps uuid[]) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
 SELECT caps IS NOT NULL AND cardinality(caps) BETWEEN 1 AND 256
  AND array_position(caps,NULL) IS NULL
  AND cardinality(caps)=(SELECT count(DISTINCT value) FROM unnest(caps) AS value)
$$;
ALTER TABLE authorization_requests ADD COLUMN resource_policy_revision bigint CHECK(resource_policy_revision>=0);
CREATE TABLE authorization_resource_grants (
 request_digest bytea PRIMARY KEY REFERENCES authorization_requests(digest) ON DELETE CASCADE,
 resource_id uuid NOT NULL REFERENCES protected_resources(id),
 principal_epoch bigint NOT NULL CHECK(principal_epoch>=0),
 capability_ceiling uuid[] NOT NULL CHECK(capability_ceiling_valid(capability_ceiling))
);
CREATE TRIGGER resource_consent_immutable BEFORE UPDATE ON authorization_resource_grants FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
ALTER TABLE authorization_codes ADD COLUMN resource_id uuid REFERENCES protected_resources(id);
ALTER TABLE authorization_codes ADD COLUMN principal_epoch bigint CHECK(principal_epoch>=0);
ALTER TABLE authorization_codes ADD COLUMN capability_ceiling uuid[] NOT NULL DEFAULT ARRAY[]::uuid[];
ALTER TABLE authorization_codes ADD CONSTRAINT code_resource_grant CHECK(
 (resource_id IS NULL AND principal_epoch IS NULL AND cardinality(capability_ceiling)=0)
 OR (resource_id IS NOT NULL AND principal_epoch IS NOT NULL AND capability_ceiling_valid(capability_ceiling)));
ALTER TABLE authorization_codes ADD UNIQUE(digest,resource_id);
ALTER TABLE authorization_codes DROP CONSTRAINT authorization_codes_scopes_check;
CREATE FUNCTION resource_token_scopes(scopes text[]) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
 SELECT scopes IS NOT NULL AND cardinality(scopes) BETWEEN 2 AND 32
 AND array_position(scopes,NULL) IS NULL AND 'openid'=ANY(scopes)
 AND NOT scopes && ARRAY['profile','email','address','phone','offline_access']
 AND cardinality(scopes)=(SELECT count(DISTINCT value) FROM unnest(scopes) AS value)
 AND NOT EXISTS(SELECT 1 FROM unnest(scopes) AS value WHERE octet_length(value) NOT BETWEEN 1 AND 100 OR value !~ '^[!#-\[\]-~]+$')
$$;
ALTER TABLE authorization_codes ADD CONSTRAINT code_scopes CHECK(
 CASE WHEN resource_id IS NULL THEN oidc_identity_scopes(scopes) ELSE resource_token_scopes(scopes) END);
ALTER TABLE access_tokens ADD COLUMN resource_id uuid REFERENCES protected_resources(id);
ALTER TABLE access_tokens ADD FOREIGN KEY(code_digest,resource_id) REFERENCES authorization_codes(digest,resource_id);
ALTER TABLE access_tokens DROP CONSTRAINT access_tokens_capability_ceiling_check;
ALTER TABLE access_tokens DROP CONSTRAINT access_scope;
ALTER TABLE access_tokens DROP CONSTRAINT access_claim_ceiling;
ALTER TABLE access_tokens ADD CONSTRAINT access_profile CHECK(
 (resource_id IS NULL AND oidc_identity_scopes(string_to_array(scope,' ')) AND claim_ceiling=oidc_identity_claims(string_to_array(scope,' ')) AND cardinality(capability_ceiling)=0)
 OR (resource_id IS NOT NULL AND resource_token_scopes(string_to_array(scope,' ')) AND cardinality(claim_ceiling)=0 AND capability_ceiling_valid(capability_ceiling) AND audience='urn:darkhorse:resource:'||resource_id::text));

CREATE FUNCTION validate_access_grant() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM authorization_codes c WHERE c.digest=NEW.code_digest
   AND c.consumed AND c.resource_id IS NOT DISTINCT FROM NEW.resource_id
   AND string_to_array(NEW.scope,' ') <@ c.scopes
   AND NEW.capability_ceiling <@ c.capability_ceiling) THEN
  RAISE EXCEPTION 'access grant exceeds authorization code' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER access_grant_binding BEFORE INSERT ON access_tokens FOR EACH ROW EXECUTE FUNCTION validate_access_grant();

ALTER TABLE consent_audit ADD COLUMN capability_ceiling uuid[] NOT NULL DEFAULT ARRAY[]::uuid[];
