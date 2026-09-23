-- An intent is durable before Redis is touched. A receipt shares the authoritative
-- activation transaction. An intent without a receipt is unresolved, never proof
-- that Redis was untouched. Database owners remain trusted.
CREATE TABLE limiter_activation_intents (
    operation_id uuid PRIMARY KEY CHECK (operation_id <> '00000000-0000-0000-0000-000000000000'),
    epoch bigint NOT NULL CHECK (epoch BETWEEN 1 AND 9007199254740991),
    generation uuid NOT NULL CHECK (generation <> '00000000-0000-0000-0000-000000000000'),
    not_before_ms bigint NOT NULL CHECK (not_before_ms BETWEEN 0 AND 9007199254740991),
    prepared_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint
        CHECK (prepared_ms BETWEEN not_before_ms AND 9007199254740991),
    database_role text NOT NULL DEFAULT session_user
);
CREATE TABLE limiter_activation_receipts (
    operation_id uuid PRIMARY KEY REFERENCES limiter_activation_intents(operation_id),
    completed_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint
        CHECK (completed_ms BETWEEN 0 AND 9007199254740991),
    run_id bytea NOT NULL CHECK (octet_length(run_id)=20),
    replication_id bytea NOT NULL CHECK (octet_length(replication_id)=20)
);
CREATE INDEX limiter_activation_recent ON limiter_activation_intents(prepared_ms DESC,operation_id);
CREATE TRIGGER immutable_activation_intent BEFORE UPDATE OR DELETE ON limiter_activation_intents FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER immutable_activation_receipt BEFORE UPDATE OR DELETE ON limiter_activation_receipts FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_activation_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM limiter_activation_intents i JOIN limiter_authority a
        ON a.singleton AND a.epoch=i.epoch AND a.generation=i.generation
        WHERE i.operation_id=NEW.operation_id AND i.database_role=session_user
        AND i.prepared_ms<=NEW.completed_ms AND a.active
        AND a.run_id=NEW.run_id AND a.replication_id=NEW.replication_id) THEN
        RAISE EXCEPTION 'invalid activation receipt' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER activation_receipt_binding BEFORE INSERT ON limiter_activation_receipts FOR EACH ROW EXECUTE FUNCTION guard_activation_receipt();
