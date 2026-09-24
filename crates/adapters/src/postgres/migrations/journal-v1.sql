-- Bookkeeping must exist before the first application migration. This schema is
-- initialized atomically with the first intent and never granted to runtime roles.
CREATE SCHEMA darkhorse_migration_v1;
REVOKE ALL ON SCHEMA darkhorse_migration_v1 FROM PUBLIC;
CREATE TABLE darkhorse_migration_v1.intents (
 operation_id uuid PRIMARY KEY CHECK(operation_id <> '00000000-0000-0000-0000-000000000000'),
 manifest_count integer NOT NULL CHECK(manifest_count BETWEEN 1 AND 128),
 baseline_count integer NOT NULL CHECK(baseline_count BETWEEN 0 AND manifest_count),
 database_role text NOT NULL DEFAULT session_user,
 prepared_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint
);
CREATE TABLE darkhorse_migration_v1.targets (
 operation_id uuid NOT NULL REFERENCES darkhorse_migration_v1.intents(operation_id),
 version bigint NOT NULL CHECK(version>0),
 checksum bytea NOT NULL CHECK(octet_length(checksum)=48),
 already_applied boolean NOT NULL,
 PRIMARY KEY(operation_id,version)
);
CREATE TABLE darkhorse_migration_v1.steps (
 operation_id uuid NOT NULL,
 version bigint NOT NULL,
 completed_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(operation_id,version),
 FOREIGN KEY(operation_id,version) REFERENCES darkhorse_migration_v1.targets(operation_id,version)
);
CREATE TABLE darkhorse_migration_v1.completions (
 operation_id uuid PRIMARY KEY REFERENCES darkhorse_migration_v1.intents(operation_id),
 completed_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint
);
CREATE INDEX migration_recent ON darkhorse_migration_v1.intents(prepared_ms DESC,operation_id);
CREATE FUNCTION darkhorse_migration_v1.immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'immutable migration evidence'; END $$;
CREATE TRIGGER immutable_intent BEFORE UPDATE OR DELETE OR TRUNCATE ON darkhorse_migration_v1.intents FOR EACH STATEMENT EXECUTE FUNCTION darkhorse_migration_v1.immutable();
CREATE TRIGGER immutable_target BEFORE UPDATE OR DELETE OR TRUNCATE ON darkhorse_migration_v1.targets FOR EACH STATEMENT EXECUTE FUNCTION darkhorse_migration_v1.immutable();
CREATE TRIGGER immutable_step BEFORE UPDATE OR DELETE OR TRUNCATE ON darkhorse_migration_v1.steps FOR EACH STATEMENT EXECUTE FUNCTION darkhorse_migration_v1.immutable();
CREATE TRIGGER immutable_completion BEFORE UPDATE OR DELETE OR TRUNCATE ON darkhorse_migration_v1.completions FOR EACH STATEMENT EXECUTE FUNCTION darkhorse_migration_v1.immutable();
REVOKE ALL ON ALL TABLES IN SCHEMA darkhorse_migration_v1 FROM PUBLIC;
REVOKE ALL ON ALL ROUTINES IN SCHEMA darkhorse_migration_v1 FROM PUBLIC;
