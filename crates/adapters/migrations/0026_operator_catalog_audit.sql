-- Catalog reads release results only after current authority and this audit commit.
-- Search text and returned catalog values are intentionally not retained here.
CREATE TABLE operator_catalog_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 operation_id uuid NOT NULL UNIQUE CHECK(operation_id<>'00000000-0000-0000-0000-000000000000'),
 command text NOT NULL CHECK(command IN ('application.list','client.list')),
 application_id uuid CHECK(application_id<>'00000000-0000-0000-0000-000000000000'),
 CHECK((command='client.list')=(application_id IS NOT NULL)),
 actor_id uuid REFERENCES principals(id),
 actor_credential_id uuid,
 actor_epoch bigint CHECK(actor_epoch>=0),
 authentication_observed_ms bigint CHECK(authentication_observed_ms>=0),
 query_limit integer NOT NULL CHECK(query_limit BETWEEN 1 AND 25),
 active_filter boolean,
 after_id uuid CHECK(after_id<>'00000000-0000-0000-0000-000000000000'),
 searched boolean NOT NULL,
 result text NOT NULL CHECK(result IN ('read','denied','not_found')),
 returned_count integer CHECK(returned_count BETWEEN 0 AND query_limit),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_credential_id,actor_id) REFERENCES credentials(id,principal_id),
 CHECK((actor_id IS NULL)=(actor_credential_id IS NULL)),
 CHECK((actor_id IS NULL)=(actor_epoch IS NULL)),
 CHECK((actor_id IS NULL)=(authentication_observed_ms IS NULL)),
 CHECK(actor_id IS NOT NULL OR result='denied'),
 CHECK((result='read')=(returned_count IS NOT NULL))
);
CREATE INDEX operator_catalog_audit_actor ON operator_catalog_audit(actor_id,id);
CREATE TRIGGER operator_catalog_audit_immutable BEFORE UPDATE OR DELETE ON operator_catalog_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
