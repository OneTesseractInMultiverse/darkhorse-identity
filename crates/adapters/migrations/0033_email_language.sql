-- Existing pending deliveries retain the original English template across retries.
ALTER TABLE email_verifications
    ADD COLUMN delivery_locale text NOT NULL DEFAULT 'en' CHECK (delivery_locale IN ('en','es')),
    ADD COLUMN template_version smallint NOT NULL DEFAULT 0,
    ADD CONSTRAINT email_template_language CHECK (template_version IN (0,1) AND (template_version<>0 OR delivery_locale='en'));
ALTER TABLE invitations
    ADD COLUMN delivery_locale text NOT NULL DEFAULT 'en' CHECK (delivery_locale IN ('en','es')),
    ADD COLUMN template_version smallint NOT NULL DEFAULT 0,
    ADD CONSTRAINT invitation_template_language CHECK (template_version IN (0,1) AND (template_version<>0 OR delivery_locale='en'));
-- Runtime insertion explicitly supplies the resolved language and current version.
-- Defaults remain legacy-compatible for older insertions during a controlled upgrade.
CREATE FUNCTION guard_delivery_presentation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id=OLD.id AND (NEW.delivery_locale,NEW.template_version)
       IS DISTINCT FROM (OLD.delivery_locale,OLD.template_version) THEN
        RAISE EXCEPTION 'immutable delivery presentation' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER email_delivery_presentation BEFORE UPDATE ON email_verifications
    FOR EACH ROW EXECUTE FUNCTION guard_delivery_presentation();
CREATE TRIGGER invitation_delivery_presentation BEFORE UPDATE ON invitations
    FOR EACH ROW EXECUTE FUNCTION guard_delivery_presentation();
