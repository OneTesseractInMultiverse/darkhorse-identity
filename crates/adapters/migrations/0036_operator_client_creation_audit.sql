-- Client issuance records authorization and completion without retaining credentials or policy.
CREATE TABLE operator_client_creation_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 application_id uuid NOT NULL CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 client_id uuid CHECK(client_id<>'00000000-0000-0000-0000-000000000000'),
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
 CHECK((result='written')=(client_id IS NOT NULL))
);
CREATE INDEX operator_client_creation_audit_actor ON operator_client_creation_audit(actor_id,id);
CREATE INDEX operator_client_creation_audit_target ON operator_client_creation_audit(application_id,client_id,id);
CREATE TRIGGER operator_client_creation_audit_immutable BEFORE UPDATE OR DELETE ON operator_client_creation_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
