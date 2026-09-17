CREATE TABLE limiter_authority (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    epoch bigint NOT NULL CHECK (epoch BETWEEN 1 AND 9007199254740991),
    generation uuid NOT NULL CHECK (generation <> '00000000-0000-0000-0000-000000000000'),
    active boolean NOT NULL,
    not_before_ms bigint NOT NULL CHECK (not_before_ms BETWEEN 0 AND 9007199254740991),
    run_id bytea,
    replication_id bytea,
    CHECK ((NOT active AND run_id IS NULL AND replication_id IS NULL) OR
           (active AND octet_length(run_id)=20 AND octet_length(replication_id)=20
            AND run_id IS NOT NULL AND replication_id IS NOT NULL))
);
CREATE TABLE limiter_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    database_role text NOT NULL DEFAULT session_user,
    epoch bigint NOT NULL,
    generation uuid NOT NULL,
    event text NOT NULL CHECK (event IN ('limiter.fenced','limiter.activated'))
);
CREATE TRIGGER retain_limiter_authority BEFORE DELETE ON limiter_authority FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER immutable_limiter_audit BEFORE UPDATE OR DELETE ON limiter_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_limiter_authority() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (NEW.epoch = OLD.epoch + 1 AND NEW.generation <> OLD.generation AND NOT NEW.active
        AND NEW.not_before_ms >= OLD.not_before_ms)
       OR (NEW.epoch = OLD.epoch AND NEW.generation = OLD.generation AND NOT OLD.active AND NEW.active
           AND NEW.not_before_ms = OLD.not_before_ms) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'invalid limiter authority transition' USING ERRCODE = '23514';
END;
$$;
CREATE TRIGGER limiter_authority_transition BEFORE UPDATE ON limiter_authority FOR EACH ROW EXECUTE FUNCTION guard_limiter_authority();
