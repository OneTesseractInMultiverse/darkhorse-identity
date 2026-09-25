-- Scoped lifecycle inspection and retirement retain no secret values or verifiers.
CREATE TABLE operator_client_secret_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN ('client.secret.list','client.secret.retire')),
 application_id uuid NOT NULL CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 client_id uuid NOT NULL CHECK(client_id<>'00000000-0000-0000-0000-000000000000'),
 secret_id uuid CHECK(secret_id<>'00000000-0000-0000-0000-000000000000'),
 expected_revision bigint CHECK(expected_revision>=0),
 target_revision bigint CHECK(target_revision>=0),
 cursor_id uuid CHECK(cursor_id<>'00000000-0000-0000-0000-000000000000'),
 page_size smallint CHECK(page_size BETWEEN 1 AND 25),
 returned_count smallint CHECK(returned_count BETWEEN 0 AND page_size),
 reason text CHECK(length(reason) BETWEEN 1 AND 200 AND octet_length(reason)<=512 AND reason !~ '[[:cntrl:]]'),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 result text NOT NULL CHECK(result IN ('read','written','denied','not_found','conflict')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied'),
 CHECK((command='client.secret.retire')=(secret_id IS NOT NULL)),
 CHECK((command='client.secret.retire')=(expected_revision IS NOT NULL)),
 CHECK((command='client.secret.retire')=(reason IS NOT NULL)),
 CHECK((command='client.secret.list')=(page_size IS NOT NULL)),
 CHECK(command='client.secret.list' OR cursor_id IS NULL),
 CHECK((result='read')=(returned_count IS NOT NULL)),
 CHECK(result<>'read' OR command='client.secret.list'),
 CHECK(result NOT IN ('written','conflict') OR command='client.secret.retire'),
 CHECK((result IN ('read','written'))=(target_revision IS NOT NULL)),
 CHECK(result<>'written' OR (target_revision>expected_revision AND target_revision-expected_revision=1))
);
CREATE INDEX operator_client_secret_audit_actor ON operator_client_secret_audit(actor_id,id);
CREATE INDEX operator_client_secret_audit_target ON operator_client_secret_audit(application_id,client_id,id);
CREATE TRIGGER operator_client_secret_audit_immutable BEFORE UPDATE OR DELETE ON operator_client_secret_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
-- Stable keyset pages include expired and retired credentials without scanning other clients.
CREATE INDEX oauth_client_secrets_inventory ON oauth_client_secrets(client_id,id);
