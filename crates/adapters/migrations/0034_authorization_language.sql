-- Presentation metadata is immutable under the existing authorization_transition trigger.
ALTER TABLE authorization_requests ADD COLUMN ui_locale text CHECK(ui_locale IN ('en','es'));
-- Existing requests have no hint; credential bindings, expiry and authority are unchanged.
