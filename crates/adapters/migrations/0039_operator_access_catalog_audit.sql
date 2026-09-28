-- Fresh-password CLI catalog writes have a distinct immutable audit from browser writes.
CREATE TABLE operator_access_catalog_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN (
   'capability.create','capability.retire','role.create','capability.binding',
   'role.binding','role.capability','resource.capability','scope.capability'
 )),
 expected_revision bigint NOT NULL CHECK(expected_revision>=0),
 resulting_revision bigint CHECK(resulting_revision>=0),
 target_id uuid CHECK(target_id<>'00000000-0000-0000-0000-000000000000'),
 application_id uuid CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 related_id uuid CHECK(related_id<>'00000000-0000-0000-0000-000000000000'),
 requested_state boolean,
 reason text NOT NULL CHECK(length(reason) BETWEEN 1 AND 200 AND octet_length(reason)<=512 AND reason !~ '[[:cntrl:]]'),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 result text NOT NULL CHECK(result IN ('changed','unchanged','denied','invalid','not_found','conflict','policy_rejected')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied'),
 CHECK((result IN ('changed','unchanged'))=(resulting_revision IS NOT NULL)),
 CHECK(result NOT IN ('changed','unchanged') OR target_id IS NOT NULL),
 CHECK(result<>'changed' OR resulting_revision>expected_revision),
 CHECK(result<>'unchanged' OR resulting_revision=expected_revision)
);
CREATE INDEX operator_access_catalog_audit_actor ON operator_access_catalog_audit(actor_id,id);
CREATE INDEX operator_access_catalog_audit_target ON operator_access_catalog_audit(target_id,id);
CREATE TRIGGER operator_access_catalog_audit_immutable BEFORE UPDATE OR DELETE ON operator_access_catalog_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
