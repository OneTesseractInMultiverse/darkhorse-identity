-- Registration history and termination intents share a transaction-ordered
-- sequence. Writers take the existing security_state fence before allocating it.
CREATE SEQUENCE logout_order AS bigint;
CREATE TABLE logout_destinations (
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 revision bigint NOT NULL CHECK(revision > 0),
 ordering bigint NOT NULL UNIQUE DEFAULT nextval('logout_order'),
 uri text CHECK(octet_length(uri) BETWEEN 9 AND 2048 AND uri LIKE 'https://%'),
 PRIMARY KEY(client_id, revision)
);
CREATE INDEX logout_destinations_snapshot ON logout_destinations(client_id,ordering DESC);
CREATE INDEX relying_party_sessions_logout ON relying_party_sessions(session_id,client_id);
CREATE TRIGGER logout_destination_immutable BEFORE UPDATE OR DELETE ON logout_destinations
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

CREATE TABLE logout_intents (
 id bigint PRIMARY KEY DEFAULT nextval('logout_order'),
 session_id uuid NOT NULL UNIQUE REFERENCES browser_sessions(public_id),
 created_ms bigint NOT NULL CHECK(created_ms >= 0),
 cursor uuid,
 expanded boolean NOT NULL DEFAULT false
);
CREATE INDEX logout_intents_pending ON logout_intents(id) WHERE NOT expanded;
CREATE INDEX logout_intents_retention ON logout_intents(created_ms, id) WHERE expanded;

CREATE TABLE logout_deliveries (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid() CHECK(id <> '00000000-0000-0000-0000-000000000000'),
 intent_id bigint NOT NULL REFERENCES logout_intents(id),
 sid uuid NOT NULL REFERENCES relying_party_sessions(sid),
 client_id uuid NOT NULL,
 revision bigint NOT NULL,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','delivered','failed','cancelled')),
 attempts smallint NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 8),
 created_ms bigint NOT NULL CHECK(created_ms >= 0),
 next_ms bigint NOT NULL CHECK(next_ms >= created_ms),
 FOREIGN KEY(client_id,revision) REFERENCES logout_destinations(client_id,revision),
 UNIQUE(intent_id,client_id)
);
CREATE INDEX logout_deliveries_due ON logout_deliveries(next_ms,id) WHERE state='queued';
CREATE INDEX logout_deliveries_retention ON logout_deliveries(created_ms,id) WHERE state<>'queued';
CREATE INDEX logout_deliveries_client ON logout_deliveries(client_id,created_ms DESC,id DESC);
CREATE TABLE logout_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 client_id uuid REFERENCES oauth_clients(id),
 actor_id uuid REFERENCES principals(id),
 delivery_id uuid,
 revision bigint CHECK(revision > 0),
 event text NOT NULL CHECK(event IN ('configured','claimed','delivered','retry','failed','cancelled')),
 occurred_ms bigint NOT NULL CHECK(occurred_ms >= 0),
 database_role text NOT NULL DEFAULT session_user
);
CREATE TRIGGER logout_audit_immutable BEFORE UPDATE OR DELETE ON logout_audit
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
