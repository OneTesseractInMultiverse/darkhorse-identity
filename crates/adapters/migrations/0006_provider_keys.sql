CREATE TABLE provider_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    issuer text COLLATE "C" NOT NULL CHECK (issuer LIKE 'https://%' AND octet_length(issuer)<=2048),
    wrap_digest bytea NOT NULL CHECK(octet_length(wrap_digest)=32),
    revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0),
    last_ms bigint NOT NULL CHECK(last_ms>=0)
);
CREATE TABLE signing_keys (
    kid text COLLATE "C" PRIMARY KEY CHECK(kid ~ '^[A-Za-z0-9_-]{43}$'),
    n text NOT NULL CHECK(octet_length(n)=512 AND n ~ '^[A-Za-z0-9_-]+$'),
    e text NOT NULL CHECK(e='AQAB'),
    nonce bytea NOT NULL UNIQUE CHECK(octet_length(nonce)=12),
    ciphertext bytea CHECK(octet_length(ciphertext) BETWEEN 32 AND 8192),
    phase text NOT NULL CHECK(phase IN ('staged','active','retiring','retired')),
    created_ms bigint NOT NULL CHECK(created_ms>=0),
    activated_ms bigint CHECK(activated_ms>=created_ms+60000),
    verify_until_ms bigint CHECK(verify_until_ms>=activated_ms+600000),
    CHECK((phase='retired')=(ciphertext IS NULL)),
    CHECK((phase IN ('staged','active') AND verify_until_ms IS NULL) OR phase IN ('retiring','retired')),
    CHECK((phase='staged' AND activated_ms IS NULL) OR (phase IN ('active','retiring') AND activated_ms IS NOT NULL) OR phase='retired'),
    CHECK(phase<>'retiring' OR verify_until_ms IS NOT NULL)
);
CREATE UNIQUE INDEX one_active_signing_key ON signing_keys(phase) WHERE phase='active';
CREATE TABLE provider_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event text NOT NULL CHECK(event IN ('key_staged','key_activated','key_retired')),
    kid text NOT NULL REFERENCES signing_keys(kid),
    revision bigint NOT NULL,
    occurred_ms bigint NOT NULL
);
CREATE TRIGGER provider_retained BEFORE DELETE ON provider_state FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER signing_keys_retained BEFORE DELETE ON signing_keys FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER provider_audit_immutable BEFORE UPDATE OR DELETE ON provider_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION provider_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.issuer<>OLD.issuer OR NEW.wrap_digest<>OLD.wrap_digest OR NEW.revision<>OLD.revision+1 OR NEW.last_ms<OLD.last_ms THEN
        RAISE EXCEPTION 'invalid provider transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER provider_transition BEFORE UPDATE ON provider_state FOR EACH ROW EXECUTE FUNCTION provider_transition();
CREATE FUNCTION signing_transition() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE t bigint;
BEGIN
    SELECT last_ms INTO t FROM provider_state WHERE singleton;
    IF NEW.kid<>OLD.kid OR NEW.n<>OLD.n OR NEW.e<>OLD.e OR NEW.nonce<>OLD.nonce OR NEW.created_ms<>OLD.created_ms
        OR (NEW.phase<>'retired' AND NEW.ciphertext IS DISTINCT FROM OLD.ciphertext)
        OR (NEW.phase<>'active' AND NEW.activated_ms IS DISTINCT FROM OLD.activated_ms)
    THEN RAISE EXCEPTION 'immutable signing material' USING ERRCODE='23514'; END IF;
    IF OLD.phase='staged' AND NEW.phase='active' AND t>=OLD.created_ms+60000 AND NEW.activated_ms=t THEN RETURN NEW; END IF;
    IF OLD.phase='active' AND NEW.phase='retiring' AND NEW.verify_until_ms=t+600000 THEN RETURN NEW; END IF;
    IF NEW.phase='retired' AND NEW.ciphertext IS NULL AND NEW.verify_until_ms IS NOT DISTINCT FROM OLD.verify_until_ms
        AND (OLD.phase='staged' OR (OLD.phase='retiring' AND t>=OLD.verify_until_ms)) THEN RETURN NEW; END IF;
    RAISE EXCEPTION 'invalid signing lifecycle' USING ERRCODE='23514';
END $$;
CREATE TRIGGER signing_transition BEFORE UPDATE ON signing_keys FOR EACH ROW EXECUTE FUNCTION signing_transition();
