-- Descriptive preference only: existing accounts retain automatic language selection.
ALTER TABLE principals ADD COLUMN preferred_locale text
 CONSTRAINT principal_preferred_locale CHECK(preferred_locale IN ('en','es'));
-- Existing reviewed principal read/update grants and immutable profile audit suffice.
-- No credential, membership, epoch, session, token or claim change is performed.
