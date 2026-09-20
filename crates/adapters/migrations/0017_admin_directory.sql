CREATE INDEX principals_directory ON principals(created_at,id);
CREATE INDEX principals_status_directory ON principals(active,created_at,id);
CREATE INDEX principals_search_email ON principals(email_key text_pattern_ops);
CREATE INDEX principals_search_name ON principals(lower(first_name||' '||last_name) text_pattern_ops);
CREATE INDEX principals_search_last_name ON principals(lower(last_name) text_pattern_ops);
CREATE TABLE directory_admin_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES principals(id),
 actor_session_id uuid NOT NULL,
 target_id uuid NOT NULL REFERENCES principals(id),
 target_revision bigint NOT NULL CHECK(target_revision>=0),
 event text NOT NULL CHECK(event IN ('names_changed','deactivated','reactivated','role_assigned','role_removed')),
 application_id uuid REFERENCES applications(id),
 role_id uuid REFERENCES roles(id),
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_session_id,actor_id) REFERENCES browser_sessions(public_id,principal_id),
 CHECK((event IN ('role_assigned','role_removed'))=(application_id IS NOT NULL AND role_id IS NOT NULL)),
 CHECK((application_id IS NULL)=(role_id IS NULL))
);
CREATE INDEX directory_admin_audit_target ON directory_admin_audit(target_id,id);
CREATE TRIGGER directory_admin_audit_immutable BEFORE UPDATE OR DELETE ON directory_admin_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
