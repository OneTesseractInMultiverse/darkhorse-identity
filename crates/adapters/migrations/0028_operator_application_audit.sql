-- Application writes share registration policy and commit both audit ledgers.
CREATE TABLE operator_application_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN ('application.create','application.update')),
 application_id uuid CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 expected_revision bigint CHECK(expected_revision>=0),
 target_revision bigint CHECK(target_revision>=0),
 owner_id uuid NOT NULL CHECK(owner_id<>'00000000-0000-0000-0000-000000000000'),
 reason text NOT NULL CHECK(length(reason) BETWEEN 1 AND 200 AND octet_length(reason)<=512 AND reason !~ '[[:cntrl:]]'),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 result text NOT NULL CHECK(result IN ('written','denied','invalid','not_found','conflict')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied'),
 CHECK((command='application.update')=(expected_revision IS NOT NULL)),
 CHECK(command='application.create' OR application_id IS NOT NULL),
 CHECK((result='written')=(target_revision IS NOT NULL)),
 CHECK(result<>'written' OR (application_id IS NOT NULL AND
   ((command='application.create' AND target_revision=0) OR
    (command='application.update' AND target_revision>expected_revision AND target_revision-expected_revision=1))))
);
CREATE INDEX operator_application_audit_actor ON operator_application_audit(actor_id,id);
CREATE INDEX operator_application_audit_target ON operator_application_audit(application_id,id);
CREATE TRIGGER operator_application_audit_immutable BEFORE UPDATE OR DELETE ON operator_application_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
