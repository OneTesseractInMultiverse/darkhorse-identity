CREATE TABLE media_configuration (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 storage_fingerprint bytea NOT NULL CHECK(octet_length(storage_fingerprint)=32)
);
CREATE TRIGGER media_configuration_immutable BEFORE UPDATE OR DELETE ON media_configuration FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TABLE media_assets (
 id uuid PRIMARY KEY CHECK(id<>'00000000-0000-0000-0000-000000000000'),
 kind text NOT NULL CHECK(kind IN ('portrait','logo','background')),
 principal_id uuid REFERENCES principals(id),
 actor_id uuid NOT NULL REFERENCES principals(id),
 state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','ready','retired')),
 created_ms bigint NOT NULL CHECK(created_ms>=0),
 cleanup_after_ms bigint NOT NULL CHECK(cleanup_after_ms>=created_ms),
 bytes integer CHECK(bytes BETWEEN 1 AND 4194304),
 digest bytea CHECK(octet_length(digest)=32),
 width integer CHECK(width BETWEEN 1 AND 2048),height integer CHECK(height BETWEEN 1 AND 2048),
 CHECK((kind='portrait')=(principal_id IS NOT NULL)),
 CHECK(state<>'ready' OR (bytes IS NOT NULL AND digest IS NOT NULL AND width IS NOT NULL AND height IS NOT NULL))
);
CREATE INDEX media_cleanup ON media_assets(cleanup_after_ms,id) WHERE state<>'ready';
CREATE INDEX media_actor_budget ON media_assets(actor_id,created_ms);
CREATE TRIGGER media_assets_retain BEFORE DELETE ON media_assets FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_media_asset() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.id<>OLD.id OR NEW.kind<>OLD.kind OR NEW.principal_id IS DISTINCT FROM OLD.principal_id OR NEW.actor_id<>OLD.actor_id OR NEW.created_ms<>OLD.created_ms OR (OLD.state='retired' AND NEW.state<>'retired') OR (OLD.state='ready' AND NEW.state='pending') OR (OLD.state<>'pending' AND (NEW.bytes IS DISTINCT FROM OLD.bytes OR NEW.digest IS DISTINCT FROM OLD.digest OR NEW.width IS DISTINCT FROM OLD.width OR NEW.height IS DISTINCT FROM OLD.height)) THEN RAISE EXCEPTION 'invalid media transition' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER media_assets_transition BEFORE UPDATE ON media_assets FOR EACH ROW EXECUTE FUNCTION guard_media_asset();
ALTER TABLE principals ADD COLUMN portrait_id uuid REFERENCES media_assets(id);
CREATE TABLE branding_settings (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0),
 logo_id uuid REFERENCES media_assets(id),background_id uuid REFERENCES media_assets(id)
);
INSERT INTO branding_settings DEFAULT VALUES;
CREATE TRIGGER branding_settings_retain BEFORE DELETE ON branding_settings FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION guard_branding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.singleton<>OLD.singleton OR NEW.revision<>OLD.revision+1 THEN RAISE EXCEPTION 'invalid branding transition' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER branding_transition BEFORE UPDATE ON branding_settings FOR EACH ROW EXECUTE FUNCTION guard_branding();
CREATE TABLE media_audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES principals(id),actor_session_id uuid NOT NULL,
 kind text NOT NULL CHECK(kind IN ('portrait','logo','background')),
 principal_id uuid REFERENCES principals(id),asset_id uuid REFERENCES media_assets(id),
 event text NOT NULL CHECK(event IN ('published','removed')),
 target_revision bigint NOT NULL CHECK(target_revision>=0),occurred_ms bigint NOT NULL CHECK(occurred_ms>=0),
 FOREIGN KEY(actor_session_id,actor_id) REFERENCES browser_sessions(public_id,principal_id),
 CHECK((kind='portrait')=(principal_id IS NOT NULL)),CHECK((event='published')=(asset_id IS NOT NULL))
);
CREATE TRIGGER media_audit_immutable BEFORE UPDATE OR DELETE ON media_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
-- Deferred checks allow publication/reference updates in one transaction, but
-- cannot commit a private image under another principal or as public branding.
CREATE FUNCTION require_media_link() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE asset media_assets; links bigint;
BEGIN
 SELECT * INTO asset FROM media_assets WHERE id=NEW.id;
 SELECT (SELECT count(*) FROM principals WHERE portrait_id=asset.id AND id=asset.principal_id AND asset.kind='portrait')
       +(SELECT count(*) FROM branding_settings WHERE (logo_id=asset.id AND asset.kind='logo') OR (background_id=asset.id AND asset.kind='background')) INTO links;
 IF (asset.state='ready' AND links<>1) OR (asset.state<>'ready' AND links<>0) THEN RAISE EXCEPTION 'invalid media reference' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER media_link_complete AFTER INSERT OR UPDATE ON media_assets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_media_link();
CREATE FUNCTION require_portrait_owner() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.portrait_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM media_assets WHERE id=NEW.portrait_id AND principal_id=NEW.id AND kind='portrait' AND state='ready') THEN RAISE EXCEPTION 'invalid portrait reference' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER portrait_owner AFTER INSERT OR UPDATE ON principals DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_portrait_owner();
CREATE FUNCTION require_branding_kind() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.logo_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM media_assets WHERE id=NEW.logo_id AND kind='logo' AND state='ready')) OR (NEW.background_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM media_assets WHERE id=NEW.background_id AND kind='background' AND state='ready')) THEN RAISE EXCEPTION 'invalid branding reference' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER branding_kind AFTER UPDATE ON branding_settings DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_branding_kind();
