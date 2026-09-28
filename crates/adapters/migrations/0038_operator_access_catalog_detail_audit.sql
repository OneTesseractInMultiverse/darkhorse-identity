-- Detailed access-catalog reads release no record until this audit commits.
-- The ledger records target identity and explicit application selection only.
CREATE TABLE operator_access_detail_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN ('resource.show','scope.show','role.show','capability.show')),
 application_id uuid CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 target_id uuid NOT NULL CHECK(target_id<>'00000000-0000-0000-0000-000000000000'),
 resource_id uuid CHECK(resource_id<>'00000000-0000-0000-0000-000000000000'),
 CHECK(command NOT IN ('resource.show','scope.show') OR application_id IS NOT NULL),
 CHECK((command='scope.show')=(resource_id IS NOT NULL)),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 result text NOT NULL CHECK(result IN ('read','denied','not_found')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied')
);
CREATE INDEX operator_access_detail_audit_actor ON operator_access_detail_audit(actor_id,id);
CREATE INDEX operator_access_detail_audit_target ON operator_access_detail_audit(command,application_id,target_id,id);
CREATE TRIGGER operator_access_detail_audit_immutable BEFORE UPDATE OR DELETE ON operator_access_detail_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
