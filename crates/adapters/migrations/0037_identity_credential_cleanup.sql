-- Ordered expiry scans stay bounded while preserving refresh-family children.
DROP INDEX access_tokens_expiry;
CREATE INDEX access_tokens_legacy_expiry
 ON access_tokens(expires_ms,digest) WHERE refresh_generation IS NULL;
CREATE INDEX authorization_codes_expiry
 ON authorization_codes(expires_ms,digest);
