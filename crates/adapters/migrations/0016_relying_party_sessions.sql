-- Preserve issuer/client/browser associations for session-specific protocol messages.
-- Existing ID tokens had no sid; do not fabricate their historical associations.
ALTER TABLE provider_state ADD UNIQUE(issuer);
CREATE TABLE relying_party_sessions (
 sid uuid PRIMARY KEY DEFAULT gen_random_uuid()
     CHECK(sid<>'00000000-0000-0000-0000-000000000000'),
 issuer text COLLATE "C" NOT NULL REFERENCES provider_state(issuer),
 session_id uuid NOT NULL,
 principal_id uuid NOT NULL,
 client_id uuid NOT NULL REFERENCES oauth_clients(id),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 database_role text NOT NULL DEFAULT session_user,
 FOREIGN KEY(session_id,principal_id) REFERENCES browser_sessions(public_id,principal_id),
 UNIQUE(issuer,session_id,client_id),
 UNIQUE(sid,principal_id,client_id)
);
CREATE INDEX relying_party_sessions_browser ON relying_party_sessions(session_id);
CREATE TRIGGER relying_party_sessions_immutable BEFORE UPDATE OR DELETE ON relying_party_sessions
 FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

-- New redemption audit rows must identify the exact association used in the ID token.
-- Historical audit rows remain unchanged, including their absent session reference.
ALTER TABLE token_audit ADD COLUMN relying_party_session_id uuid;
ALTER TABLE token_audit ADD FOREIGN KEY(relying_party_session_id,principal_id,client_id)
 REFERENCES relying_party_sessions(sid,principal_id,client_id);
ALTER TABLE token_audit ADD CHECK(relying_party_session_id IS NULL OR event='code_redeemed');
CREATE FUNCTION token_redemption_session_required() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.event='code_redeemed' AND NEW.relying_party_session_id IS NULL THEN
  RAISE EXCEPTION 'redemption session required' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER token_redemption_session_required BEFORE INSERT ON token_audit
 FOR EACH ROW EXECUTE FUNCTION token_redemption_session_required();
