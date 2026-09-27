-- The bounded cleanup orders by expiration and digest. Match both sort keys so
-- equal-expiry backlogs do not require an incremental sort of the whole batch.
DROP INDEX authorization_expiry;
CREATE INDEX authorization_expiry ON authorization_requests(expires_ms, digest);
