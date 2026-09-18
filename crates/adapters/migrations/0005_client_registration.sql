CREATE TABLE applications (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    owner_id uuid NOT NULL REFERENCES principals(id),
    active boolean NOT NULL,
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
CREATE TABLE protected_resources (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    application_id uuid NOT NULL REFERENCES applications(id),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    audience text NOT NULL UNIQUE CHECK (audience = 'urn:darkhorse:resource:' || id::text),
    UNIQUE (application_id, id)
);
CREATE TABLE resource_scopes (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    application_id uuid NOT NULL,
    resource_id uuid NOT NULL,
    name text COLLATE "C" NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 100 AND name ~ '^[!#-\[\]-~]+$'
        AND name NOT IN ('openid','profile','email','address','phone','offline_access')),
    FOREIGN KEY (application_id, resource_id) REFERENCES protected_resources(application_id, id),
    UNIQUE (resource_id, name),
    UNIQUE (application_id, resource_id, id)
);
CREATE TABLE oauth_clients (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    application_id uuid NOT NULL REFERENCES applications(id),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    active boolean NOT NULL,
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    authentication_method text NOT NULL DEFAULT 'client_secret_basic' CHECK (authentication_method='client_secret_basic'),
    UNIQUE (application_id, id)
);
CREATE TABLE client_redirects (
    client_id uuid NOT NULL REFERENCES oauth_clients(id),
    uri text COLLATE "C" NOT NULL CHECK (octet_length(uri) BETWEEN 1 AND 2048 AND uri LIKE 'https://%' AND uri !~ '[[:space:]#*\\]'),
    PRIMARY KEY (client_id, uri)
);
CREATE TABLE client_resources (
    application_id uuid NOT NULL,
    client_id uuid NOT NULL,
    resource_id uuid NOT NULL,
    PRIMARY KEY (client_id, resource_id),
    UNIQUE (application_id, client_id, resource_id),
    FOREIGN KEY (application_id, client_id) REFERENCES oauth_clients(application_id,id),
    FOREIGN KEY (application_id, resource_id) REFERENCES protected_resources(application_id,id)
);
CREATE TABLE client_scopes (
    application_id uuid NOT NULL,
    client_id uuid NOT NULL,
    resource_id uuid NOT NULL,
    scope_id uuid NOT NULL,
    PRIMARY KEY (client_id, scope_id),
    FOREIGN KEY (application_id, client_id, resource_id) REFERENCES client_resources(application_id,client_id,resource_id),
    FOREIGN KEY (application_id, resource_id, scope_id) REFERENCES resource_scopes(application_id,resource_id,id)
);
CREATE TABLE oauth_client_secrets (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    client_id uuid NOT NULL REFERENCES oauth_clients(id),
    verifier bytea NOT NULL UNIQUE CHECK (octet_length(verifier)=32),
    created_ms bigint NOT NULL CHECK (created_ms>=0),
    expires_ms bigint CHECK (expires_ms>=created_ms),
    retired boolean NOT NULL DEFAULT false
);
CREATE UNIQUE INDEX one_current_client_secret ON oauth_client_secrets(client_id) WHERE expires_ms IS NULL AND NOT retired;
CREATE UNIQUE INDEX one_overlapping_client_secret ON oauth_client_secrets(client_id) WHERE expires_ms IS NOT NULL AND NOT retired;
CREATE TABLE registration_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id uuid NOT NULL REFERENCES principals(id),
    target_id uuid NOT NULL,
    event text NOT NULL CHECK (event IN ('application_created','application_updated','resource_created','scope_created','client_created','client_updated','secret_rotated','secret_retired')),
    occurred_ms bigint NOT NULL CHECK (occurred_ms>=0)
);
CREATE TRIGGER registration_audit_immutable BEFORE UPDATE OR DELETE ON registration_audit FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER resources_immutable BEFORE UPDATE OR DELETE ON protected_resources FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER scopes_immutable BEFORE UPDATE OR DELETE ON resource_scopes FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER applications_retained BEFORE DELETE ON applications FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER clients_retained BEFORE DELETE ON oauth_clients FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER client_secrets_retained BEFORE DELETE ON oauth_client_secrets FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE FUNCTION registration_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id<>OLD.id OR NEW.revision<>OLD.revision+1 THEN
        RAISE EXCEPTION 'invalid registration transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER applications_revision BEFORE UPDATE ON applications FOR EACH ROW EXECUTE FUNCTION registration_revision();
CREATE TRIGGER clients_revision BEFORE UPDATE ON oauth_clients FOR EACH ROW EXECUTE FUNCTION registration_revision();
CREATE FUNCTION client_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.application_id<>OLD.application_id OR NEW.authentication_method<>OLD.authentication_method THEN
        RAISE EXCEPTION 'immutable client identity' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER clients_identity BEFORE UPDATE ON oauth_clients FOR EACH ROW EXECUTE FUNCTION client_transition();
CREATE FUNCTION client_secret_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id<>OLD.id OR NEW.client_id<>OLD.client_id OR NEW.verifier<>OLD.verifier OR NEW.created_ms<>OLD.created_ms
        OR (OLD.retired AND NOT NEW.retired)
        OR (OLD.expires_ms IS NOT NULL AND (NEW.expires_ms IS NULL OR NEW.expires_ms<>OLD.expires_ms))
        OR (NEW.expires_ms IS NOT NULL AND NEW.expires_ms > floor(extract(epoch FROM clock_timestamp())*1000)::bigint + 300000)
    THEN
        RAISE EXCEPTION 'invalid client secret transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER client_secrets_transition BEFORE UPDATE ON oauth_client_secrets FOR EACH ROW EXECUTE FUNCTION client_secret_transition();
CREATE TRIGGER applications_policy AFTER INSERT OR UPDATE ON applications FOR EACH ROW EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER resources_policy AFTER INSERT ON protected_resources FOR EACH ROW EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER scopes_policy AFTER INSERT ON resource_scopes FOR EACH ROW EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER clients_policy AFTER INSERT OR UPDATE ON oauth_clients FOR EACH ROW EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER client_secrets_policy AFTER INSERT OR UPDATE ON oauth_client_secrets FOR EACH ROW EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER client_redirects_policy AFTER INSERT OR UPDATE OR DELETE ON client_redirects FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER client_resources_policy AFTER INSERT OR UPDATE OR DELETE ON client_resources FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
CREATE TRIGGER client_scopes_policy AFTER INSERT OR UPDATE OR DELETE ON client_scopes FOR EACH STATEMENT EXECUTE FUNCTION bump_policy_revision();
