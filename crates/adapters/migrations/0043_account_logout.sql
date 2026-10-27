-- Account invalidation records one root under the existing security writer fence.
-- Session expansion happens later; newly issued epochs never match this root.
CREATE TABLE logout_account_intents (
 id bigint PRIMARY KEY DEFAULT nextval('logout_order'),
 principal_id uuid NOT NULL REFERENCES principals(id),
 credential_epoch bigint NOT NULL CHECK(credential_epoch >= 0),
 created_ms bigint NOT NULL CHECK(created_ms >= 0),
 cursor uuid,
 expanded boolean NOT NULL DEFAULT false,
 UNIQUE(principal_id,credential_epoch)
);
CREATE INDEX logout_account_intents_pending ON logout_account_intents(id) WHERE NOT expanded;
CREATE INDEX logout_account_intents_retention ON logout_account_intents(created_ms,id) WHERE expanded;
CREATE INDEX browser_sessions_logout_epoch ON browser_sessions(principal_id,credential_epoch,public_id);

-- A lazily created child retains the registration snapshot of the original
-- transition. Existing individual-session intents retain their own ordering.
ALTER TABLE logout_intents ADD COLUMN destination_order bigint
 CHECK(destination_order > 0 AND destination_order < id);
