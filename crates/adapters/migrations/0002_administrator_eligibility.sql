-- Eligibility is deliberately limited to the password kind implemented today.
-- New credential/assurance workflows must extend this predicate atomically.
CREATE VIEW eligible_administrators AS
SELECT a.principal_id
FROM platform_administrators a
JOIN principals p ON p.id = a.principal_id
WHERE p.active AND EXISTS (
    SELECT 1 FROM credentials c
    JOIN password_credentials pc ON pc.credential_id = c.id
    WHERE c.principal_id = p.id AND c.kind = 'password' AND NOT c.revoked
);

-- Refuse to upgrade a previously bootstrapped, unusable directory silently.
DO $$ BEGIN
    IF (SELECT bootstrapped FROM security_state WHERE singleton)
       AND NOT EXISTS (SELECT 1 FROM eligible_administrators) THEN
        RAISE EXCEPTION 'no eligible administrator' USING ERRCODE = '23514';
    END IF;
END; $$;

CREATE OR REPLACE FUNCTION require_active_administrator() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM singleton FROM security_state WHERE singleton FOR UPDATE;
    IF (SELECT bootstrapped FROM security_state WHERE singleton)
       AND NOT EXISTS (SELECT 1 FROM eligible_administrators) THEN
        RAISE EXCEPTION 'last eligible administrator' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER bootstrap_administrator_guard AFTER UPDATE ON security_state DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_active_administrator();
CREATE CONSTRAINT TRIGGER credential_administrator_guard AFTER UPDATE ON credentials DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_active_administrator();
CREATE CONSTRAINT TRIGGER password_administrator_guard AFTER UPDATE OR DELETE ON password_credentials DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_active_administrator();
CREATE TRIGGER credential_policy_revision AFTER INSERT OR UPDATE ON credentials FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER password_policy_revision AFTER INSERT OR UPDATE OR DELETE ON password_credentials FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
