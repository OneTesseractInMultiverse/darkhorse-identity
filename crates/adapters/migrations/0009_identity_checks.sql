-- Existing codes/tokens retain their original sub-only disclosure ceiling.
CREATE FUNCTION oidc_identity_scopes(scopes text[]) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
 SELECT scopes IS NOT NULL AND cardinality(scopes) BETWEEN 1 AND 3
    AND array_position(scopes,NULL) IS NULL AND 'openid'=ANY(scopes)
    AND scopes <@ ARRAY['openid','profile','email']
    AND cardinality(scopes)=(SELECT count(DISTINCT s) FROM unnest(scopes) AS s)
$$;
CREATE FUNCTION oidc_identity_claims(scopes text[]) RETURNS text[] LANGUAGE sql IMMUTABLE AS $$
 SELECT ARRAY['sub']
    || CASE WHEN 'profile'=ANY(scopes) THEN ARRAY['name','given_name','family_name'] ELSE ARRAY[]::text[] END
    || CASE WHEN 'email'=ANY(scopes) THEN ARRAY['email','email_verified'] ELSE ARRAY[]::text[] END
$$;
ALTER TABLE authorization_codes ADD COLUMN scopes text[] NOT NULL DEFAULT ARRAY['openid'] CHECK (oidc_identity_scopes(scopes));
ALTER TABLE access_tokens DROP CONSTRAINT access_tokens_scope_check;
ALTER TABLE access_tokens DROP CONSTRAINT access_tokens_claim_ceiling_check;
ALTER TABLE access_tokens ADD CONSTRAINT access_scope CHECK (oidc_identity_scopes(string_to_array(scope,' ')));
ALTER TABLE access_tokens ADD CONSTRAINT access_claim_ceiling CHECK (claim_ceiling=oidc_identity_claims(string_to_array(scope,' ')));
ALTER TABLE token_audit DROP CONSTRAINT token_audit_event_check;
ALTER TABLE token_audit ADD CONSTRAINT token_audit_event_check CHECK (event IN ('code_issued','code_redeemed','code_replayed','access_revoked'));
