CREATE INDEX applications_directory_name ON applications(lower(name) text_pattern_ops);
CREATE INDEX clients_directory_name ON oauth_clients(application_id,lower(name) text_pattern_ops,id);
CREATE INDEX resources_directory ON protected_resources(application_id,id);
CREATE INDEX scopes_directory ON resource_scopes(application_id,id);
CREATE INDEX capabilities_directory_key ON capabilities(lower(permission_key) text_pattern_ops);
CREATE INDEX roles_directory_name ON roles(lower(name) text_pattern_ops);
CREATE TABLE catalog_admin_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES principals(id),
 actor_session_id uuid NOT NULL,
 policy_revision bigint NOT NULL CHECK(policy_revision>=0),
 event text NOT NULL CHECK(event IN ('capability_created','capability_retired','role_created','capability_bound','capability_unbound','role_bound','role_unbound','role_capability_granted','role_capability_removed','resource_capability_exposed','resource_capability_removed','scope_capability_included','scope_capability_removed')),
 target_id uuid NOT NULL,
 application_id uuid REFERENCES applications(id),
 related_id uuid,
 occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(actor_session_id,actor_id) REFERENCES browser_sessions(public_id,principal_id)
);
CREATE INDEX catalog_admin_audit_target ON catalog_admin_audit(target_id,id);
CREATE TRIGGER catalog_admin_audit_immutable BEFORE UPDATE OR DELETE ON catalog_admin_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
