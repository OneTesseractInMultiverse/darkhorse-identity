-- One deployment-wide issuance policy, read from the primary on every creation.
CREATE TABLE personal_key_policy (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 default_days smallint NOT NULL CHECK(default_days BETWEEN 1 AND 3650),
 maximum_days smallint NOT NULL CHECK(maximum_days BETWEEN default_days AND 3650),
 allow_never boolean NOT NULL
);
INSERT INTO personal_key_policy VALUES(true,30,365,true);
CREATE TRIGGER personal_key_policy_revision BEFORE UPDATE ON personal_key_policy FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER personal_key_policy_audit AFTER UPDATE ON personal_key_policy FOR EACH ROW EXECUTE FUNCTION audit_authorization_policy();
CREATE TRIGGER retain_personal_key_policy BEFORE DELETE ON personal_key_policy FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

ALTER TABLE credentials ADD UNIQUE(id,principal_id);
CREATE TABLE personal_keys (
 credential_id uuid PRIMARY KEY,
 principal_id uuid NOT NULL,
 kind text NOT NULL DEFAULT 'personal_key' CHECK(kind='personal_key'),
 application_id uuid NOT NULL REFERENCES applications(id),
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 100 AND name=btrim(name) AND name !~ '[[:cntrl:]]'),
 principal_epoch bigint NOT NULL CHECK(principal_epoch>=0),
 policy_revision bigint NOT NULL CHECK(policy_revision>=0),
 created_ms bigint NOT NULL CHECK(created_ms BETWEEN 0 AND 8640000000000000),
 expires_ms bigint CHECK(expires_ms>created_ms AND expires_ms<=8640000000000000),
 FOREIGN KEY(credential_id,principal_id) REFERENCES credentials(id,principal_id),
 FOREIGN KEY(credential_id,kind) REFERENCES credentials(id,kind),
 UNIQUE(credential_id,application_id),
 UNIQUE(credential_id,principal_id)
);
CREATE INDEX personal_keys_owner ON personal_keys(principal_id,credential_id);
CREATE INDEX personal_keys_issuance ON personal_keys(principal_id,created_ms);
CREATE TABLE personal_key_verifiers (
 credential_id uuid PRIMARY KEY REFERENCES personal_keys(credential_id),
 verifier bytea UNIQUE NOT NULL CHECK(octet_length(verifier)=32),
 scheme text NOT NULL DEFAULT 'sha256-v1' CHECK(scheme='sha256-v1')
);
CREATE TABLE personal_key_grants (
 credential_id uuid NOT NULL,
 application_id uuid NOT NULL,
 resource_id uuid NOT NULL,
 capability_ceiling uuid[] NOT NULL CHECK(capability_ceiling_valid(capability_ceiling)),
 PRIMARY KEY(credential_id,resource_id),
 FOREIGN KEY(credential_id,application_id) REFERENCES personal_keys(credential_id,application_id),
 FOREIGN KEY(application_id,resource_id) REFERENCES protected_resources(application_id,id)
);
CREATE TABLE personal_key_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 credential_id uuid NOT NULL,
 principal_id uuid NOT NULL,
 actor_session_id uuid NOT NULL,
 event text NOT NULL CHECK(event IN ('issued','revoked')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 UNIQUE(credential_id,event),
 FOREIGN KEY(credential_id,principal_id) REFERENCES personal_keys(credential_id,principal_id),
 FOREIGN KEY(actor_session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id)
);
CREATE TRIGGER personal_key_immutable BEFORE UPDATE OR DELETE ON personal_keys FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER personal_key_verifier_immutable BEFORE UPDATE OR DELETE ON personal_key_verifiers FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER personal_key_grant_immutable BEFORE UPDATE OR DELETE ON personal_key_grants FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER personal_key_audit_immutable BEFORE UPDATE OR DELETE ON personal_key_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION personal_key_sealed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM personal_key_audit WHERE credential_id=NEW.credential_id AND event='issued') THEN
  RAISE EXCEPTION 'personal key grants are sealed' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER personal_key_grant_sealed BEFORE INSERT ON personal_key_grants FOR EACH ROW EXECUTE FUNCTION personal_key_sealed();
CREATE FUNCTION personal_key_complete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM personal_key_verifiers WHERE credential_id=NEW.credential_id)
 OR NOT EXISTS(SELECT 1 FROM personal_key_audit WHERE credential_id=NEW.credential_id AND event='issued')
 OR (SELECT count(*) FROM personal_key_grants WHERE credential_id=NEW.credential_id) NOT BETWEEN 1 AND 16 THEN
  RAISE EXCEPTION 'incomplete personal key' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER personal_key_complete AFTER INSERT ON personal_keys DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION personal_key_complete();
