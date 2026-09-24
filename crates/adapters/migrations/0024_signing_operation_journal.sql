-- Intents identify deployment credentials, not individual operators. Receipts
-- reference the lifecycle audit inserted in the same application transaction.
CREATE TABLE signing_operation_intents (
    operation_id uuid PRIMARY KEY CHECK(operation_id <> '00000000-0000-0000-0000-000000000000'),
    operation text NOT NULL CHECK(operation IN ('generate','import','activate','retire')),
    issuer text COLLATE "C" NOT NULL CHECK(issuer LIKE 'https://%' AND octet_length(issuer)<=2048),
    kid text COLLATE "C" NOT NULL CHECK(kid ~ '^[A-Za-z0-9_-]{43}$'),
    expected_revision bigint NOT NULL CHECK(expected_revision BETWEEN 0 AND 9223372036854775806),
    prepared_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint CHECK(prepared_ms>=0),
    database_role text NOT NULL DEFAULT session_user
);
CREATE TABLE signing_operation_receipts (
    operation_id uuid PRIMARY KEY REFERENCES signing_operation_intents(operation_id),
    audit_id bigint NOT NULL UNIQUE REFERENCES provider_audit(id),
    completed_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp())*1000)::bigint CHECK(completed_ms>=0)
);
CREATE INDEX signing_operation_recent ON signing_operation_intents(prepared_ms DESC,operation_id);
CREATE TRIGGER immutable_signing_intent BEFORE UPDATE OR DELETE ON signing_operation_intents FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER immutable_signing_receipt BEFORE UPDATE OR DELETE ON signing_operation_receipts FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_signing_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM signing_operation_intents i
        JOIN provider_audit a ON a.id=NEW.audit_id AND a.kid=i.kid AND a.revision=i.expected_revision+1
        JOIN provider_state p ON p.singleton AND p.issuer=i.issuer AND p.revision=a.revision AND p.last_ms=a.occurred_ms
        WHERE i.operation_id=NEW.operation_id AND i.database_role=session_user
        AND i.prepared_ms<=a.occurred_ms AND a.occurred_ms<=NEW.completed_ms
        AND a.event=CASE i.operation WHEN 'generate' THEN 'key_staged' WHEN 'import' THEN 'key_staged'
            WHEN 'activate' THEN 'key_activated' WHEN 'retire' THEN 'key_retired' END) THEN
        RAISE EXCEPTION 'invalid signing receipt' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER signing_receipt_binding BEFORE INSERT ON signing_operation_receipts FOR EACH ROW EXECUTE FUNCTION guard_signing_receipt();
