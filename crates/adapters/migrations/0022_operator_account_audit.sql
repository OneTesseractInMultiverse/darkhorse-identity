-- Account CLI authentication has no browser session or reusable operator token.
CREATE TABLE operator_account_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN ('account.show','account.deactivate','account.reactivate','account.revoke_all')),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 -- A request identifier is untrusted until authorization and target lookup.
 target_id uuid NOT NULL CHECK(target_id<>'00000000-0000-0000-0000-000000000000'),
 expected_revision bigint CHECK(expected_revision>=0),
 target_revision bigint CHECK(target_revision>=0),
 reason text CHECK(length(reason) BETWEEN 1 AND 200 AND octet_length(reason)<=512 AND reason !~ '[[:cntrl:]]'),
 result text NOT NULL CHECK(result IN ('read','changed','unchanged','denied','not_found','conflict','policy_rejected')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied'),
 CHECK(command='account.show' OR (reason IS NOT NULL AND expected_revision IS NOT NULL)),
 CHECK((result IN ('read','changed','unchanged'))=(target_revision IS NOT NULL))
);
CREATE INDEX operator_account_audit_actor ON operator_account_audit(actor_id,id);
CREATE INDEX operator_account_audit_target ON operator_account_audit(target_id,id);
CREATE TRIGGER operator_account_audit_immutable BEFORE UPDATE OR DELETE ON operator_account_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
