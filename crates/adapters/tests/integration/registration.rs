use super::*;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use darkhorse_adapters::{
    registration::{OsRegistrationEntropy, secret_digest},
    registration_http,
};
use darkhorse_application::{authentication::AuthenticationStore, registration::*};
use darkhorse_domain::{identity::*, registration::*};
use tower::ServiceExt;

#[tokio::test]
async fn store_rejects_invalid_direct_inputs_and_overlarge_persisted_allowances() {
    let f = Fixture::new().await;
    let app = f.app().await;
    let mut spec = client_spec();
    spec.resources = vec![ResourceId::from_u128(99).unwrap(); 2];
    assert!(matches!(
        f.registry
            .write(
                [1; 32],
                Command::CreateClient {
                    application: app.id,
                    spec
                }
            )
            .await,
        Err(RegistrationError::Invalid)
    ));
    let (client, secret) = f.client(app.id, client_spec()).await;
    // Simulate stored data outside the adapter's bounded grant contract.
    let ids = (1000..1033).map(Uuid::from_u128).collect::<Vec<_>>();
    sqlx::query("INSERT INTO protected_resources(id,application_id,name,audience) SELECT id,$1,'API','urn:darkhorse:resource:'||id::text FROM unnest($2::uuid[]) id")
        .bind(Uuid::from_u128(app.id.as_u128())).bind(&ids).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO client_resources(application_id,client_id,resource_id) SELECT $1,$2,unnest($3::uuid[])")
        .bind(Uuid::from_u128(app.id.as_u128())).bind(Uuid::from_u128(client.id.as_u128())).bind(&ids).execute(&f.db.pool).await.unwrap();
    assert!(matches!(
        f.registry
            .read(
                [1; 32],
                ReadTarget::Client {
                    application: app.id,
                    client: client.id
                }
            )
            .await,
        Err(RegistrationError::Invalid)
    ));
    assert!(!authenticates(&f, client.id, &secret).await);
}

#[tokio::test]
async fn database_rejection_and_write_failure_never_disclose_or_commit_a_secret() {
    let f = Fixture::new().await;
    let app = f.app().await;
    sqlx::query("CREATE FUNCTION reject_client_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected write failure' USING ERRCODE=TG_ARGV[0]; END $$").execute(&f.db.pool).await.unwrap();
    for (query, expected) in [
        (
            "CREATE TRIGGER reject_client BEFORE INSERT ON oauth_clients FOR EACH ROW EXECUTE FUNCTION reject_client_write('23514')",
            RegistrationError::Invalid,
        ),
        (
            "CREATE TRIGGER reject_client BEFORE INSERT ON oauth_clients FOR EACH ROW EXECUTE FUNCTION reject_client_write('P0001')",
            RegistrationError::Unavailable,
        ),
    ] {
        sqlx::query(query).execute(&f.db.pool).await.unwrap();
        assert!(
            matches!(f.registry.write([1;32],Command::CreateClient{application:app.id,spec:client_spec()}).await,Err(error) if error==expected)
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_clients")
                .fetch_one(&f.db.pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_client_secrets")
                .fetch_one(&f.db.pool)
                .await
                .unwrap(),
            0
        );
        sqlx::query("DROP TRIGGER reject_client ON oauth_clients")
            .execute(&f.db.pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn database_constraints_preserve_identity_grant_binding_and_terminal_secret_retirement() {
    let f = Fixture::new().await;
    let a = f.app().await;
    let b = f.app().await;
    let (client, _) = f.client(a.id, client_spec()).await;
    let Record::Resource(resource) = f
        .write(Command::CreateResource {
            application: b.id,
            name: Label::new("Other API").unwrap(),
        })
        .await
        .record
    else {
        panic!("resource")
    };
    let Record::Scope(scope) = f
        .write(Command::CreateScope {
            application: b.id,
            resource: resource.id,
            name: ScopeName::new("read").unwrap(),
        })
        .await
        .record
    else {
        panic!("scope")
    };
    for query in [
        "UPDATE applications SET id='00000000-0000-0000-0000-000000000099',revision=revision+1",
        "UPDATE applications SET revision=revision+2",
        "DELETE FROM applications",
        "UPDATE protected_resources SET audience='different'",
        "DELETE FROM protected_resources",
        "UPDATE resource_scopes SET name='different'",
        "DELETE FROM resource_scopes",
        "UPDATE oauth_clients SET revision=revision+2",
        "UPDATE oauth_clients SET authentication_method='client_secret_post',revision=revision+1",
        "UPDATE oauth_clients SET application_id='00000000-0000-0000-0000-000000000099',revision=revision+1",
        "DELETE FROM oauth_clients",
        "UPDATE oauth_client_secrets SET verifier=decode(repeat('ff',32),'hex')",
        "UPDATE oauth_client_secrets SET created_ms=created_ms+1",
        "DELETE FROM oauth_client_secrets",
        "UPDATE registration_audit SET event='client_created'",
        "DELETE FROM registration_audit",
    ] {
        assert!(
            sqlx::query(query).execute(&f.db.pool).await.is_err(),
            "{query}"
        );
    }
    assert!(
        sqlx::query(
            "INSERT INTO client_resources(application_id,client_id,resource_id) VALUES($1,$2,$3)"
        )
        .bind(Uuid::from_u128(a.id.as_u128()))
        .bind(Uuid::from_u128(client.id.as_u128()))
        .bind(Uuid::from_u128(resource.id.as_u128()))
        .execute(&f.db.pool)
        .await
        .is_err()
    );
    assert!(sqlx::query("INSERT INTO client_scopes(application_id,client_id,resource_id,scope_id) VALUES($1,$2,$3,$4)")
        .bind(Uuid::from_u128(a.id.as_u128())).bind(Uuid::from_u128(client.id.as_u128())).bind(Uuid::from_u128(resource.id.as_u128())).bind(Uuid::from_u128(scope.id.as_u128())).execute(&f.db.pool).await.is_err());
    for uri in [
        "https://*.example/cb",
        "https://app.example/cb#fragment",
        "http://app.example/cb",
        "https://app.example/ space",
    ] {
        assert!(
            sqlx::query("INSERT INTO client_redirects(client_id,uri) VALUES($1,$2)")
                .bind(Uuid::from_u128(client.id.as_u128()))
                .bind(uri)
                .execute(&f.db.pool)
                .await
                .is_err()
        );
    }
    assert!(
        sqlx::query(
            "INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES($1,$2,$3,0)"
        )
        .bind(Uuid::from_u128(999))
        .bind(Uuid::from_u128(client.id.as_u128()))
        .bind([9u8; 32].as_slice())
        .execute(&f.db.pool)
        .await
        .is_err()
    );
    f.write(rotate(a.id, client.id, 0, 300)).await;
    assert!(
        sqlx::query("UPDATE oauth_client_secrets SET expires_ms=NULL WHERE expires_ms IS NOT NULL")
            .execute(&f.db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query(
            "UPDATE oauth_client_secrets SET expires_ms=expires_ms+1 WHERE expires_ms IS NOT NULL"
        )
        .execute(&f.db.pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "UPDATE oauth_client_secrets SET expires_ms=created_ms+600000 WHERE expires_ms IS NULL"
        )
        .execute(&f.db.pool)
        .await
        .is_err()
    );
    f.write(rotate(a.id, client.id, 1, 0)).await;
    assert!(
        sqlx::query("UPDATE oauth_client_secrets SET retired=false WHERE retired")
            .execute(&f.db.pool)
            .await
            .is_err()
    );
    let duplicate = f
        .registry
        .write(
            [1; 32],
            Command::CreateScope {
                application: b.id,
                resource: resource.id,
                name: ScopeName::new("read").unwrap(),
            },
        )
        .await;
    assert!(matches!(duplicate, Err(RegistrationError::Conflict)));
}

#[tokio::test]
async fn lost_administrator_role_and_inactive_owner_are_revalidated() {
    let f = Fixture::new().await;
    insert_principal(&f.db, 2, true).await;
    let command = Command::CreateApplication(app_spec(true));
    f.db.store.preflight([1; 32], &command).await.unwrap();
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(Uuid::from_u128(1))
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.db.store
            .execute(
                [1; 32],
                &command,
                Prepared {
                    identifier: Some(OsRegistrationEntropy.identifier().unwrap()),
                    secret: None
                }
            )
            .await,
        Err(RegistrationError::Forbidden)
    ));
    sqlx::query("INSERT INTO platform_administrators(principal_id) VALUES($1)")
        .bind(Uuid::from_u128(1))
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut spec = app_spec(true);
    spec.owner = id(2);
    let command = Command::CreateApplication(spec);
    f.db.store.preflight([1; 32], &command).await.unwrap();
    f.db.store
        .change(id(2), 0, AccountAction::SetStatus(AccountStatus::Inactive))
        .await
        .unwrap();
    assert!(matches!(
        f.db.store
            .execute(
                [1; 32],
                &command,
                Prepared {
                    identifier: Some(OsRegistrationEntropy.identifier().unwrap()),
                    secret: None
                }
            )
            .await,
        Err(RegistrationError::Invalid)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM applications")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn real_transport_uses_the_browser_actor_and_reveals_secrets_only_at_issuance() {
    let f = Fixture::new().await;
    let candidate =
        f.db.store
            .candidate("admin@example.com")
            .await
            .unwrap()
            .unwrap();
    let raw = "a".repeat(64);
    use sha2::{Digest, Sha256};
    f.db.store
        .establish(&candidate, Sha256::digest(raw.as_bytes()).into(), None)
        .await
        .unwrap();
    let app = f.app().await;
    let router = registration_http::router(
        f.registry,
        url::Url::parse("https://localhost:8443").unwrap(),
    );
    let request = |method: &str, path: &str, body: String| {
        Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost:8443")
            .header("origin", "https://localhost:8443")
            .header("x-darkhorse-csrf", "1")
            .header("cookie", format!("__Host-darkhorse={raw}"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let payload = serde_json::json!({"operation":"create_client","application_id":Uuid::from_u128(app.id.as_u128()).to_string(),"client":{"name":"Web","active":true,"redirect_uris":["https://app.example/cb"],"resource_ids":[],"scope_ids":[],"token_endpoint_auth_method":"client_secret_basic"}});
    let response = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/admin/registration",
            payload.to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let secret = created["client_secret"].as_str().unwrap();
    let client = created["record"]["id"].as_str().unwrap();
    assert!(
        f.db.store
            .authenticate_client(
                ClientId::from_u128(Uuid::parse_str(client).unwrap().as_u128()).unwrap(),
                secret_digest(secret).unwrap()
            )
            .await
            .is_ok()
    );
    let path = format!(
        "/api/admin/applications/{}/clients/{client}",
        Uuid::from_u128(app.id.as_u128())
    );
    let response = router
        .clone()
        .oneshot(request("GET", &path, String::new()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(secret));
    assert!(!String::from_utf8_lossy(&bytes).contains("verifier"));
    f.db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let response = router
        .oneshot(request(
            "POST",
            "/api/admin/registration",
            payload.to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_clients")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        1
    );
}
type Registry = Service<PostgresStore, OsRegistrationEntropy>;
struct Fixture {
    db: Database,
    registry: Registry,
}
impl Fixture {
    async fn new() -> Self {
        let db = Database::new().await;
        db.store
            .bootstrap(super::administrator(1, "admin@example.com"))
            .await
            .unwrap();
        let candidate = db
            .store
            .candidate("admin@example.com")
            .await
            .unwrap()
            .unwrap();
        db.store.establish(&candidate, [1; 32], None).await.unwrap();
        Self {
            registry: Service {
                store: db.store.clone(),
                entropy: OsRegistrationEntropy,
            },
            db,
        }
    }
    async fn write(&self, command: Command) -> Written {
        self.registry.write([1; 32], command).await.unwrap()
    }
    async fn app(&self) -> ApplicationRecord {
        let Record::Application(record) = self
            .write(Command::CreateApplication(app_spec(true)))
            .await
            .record
        else {
            panic!("application")
        };
        record
    }
    async fn client(&self, app: ApplicationId, spec: ClientSpec) -> (ClientRecord, String) {
        let result = self
            .write(Command::CreateClient {
                application: app,
                spec,
            })
            .await;
        let Record::Client(record) = result.record else {
            panic!("client")
        };
        (record, result.secret.unwrap())
    }
}
fn app_spec(active: bool) -> ApplicationSpec {
    ApplicationSpec {
        name: Label::new("Portal").unwrap(),
        owner: id(1),
        active,
    }
}
fn client_spec() -> ClientSpec {
    ClientSpec::new(
        Label::new("Web").unwrap(),
        true,
        darkhorse_adapters::registration::redirects(vec![
            "https://app.example/callback?tenant=1".into(),
        ])
        .unwrap(),
        vec![],
        vec![],
        "client_secret_basic",
    )
    .unwrap()
}
fn client_record(written: Written) -> (ClientRecord, Option<String>) {
    let Record::Client(record) = written.record else {
        panic!("client")
    };
    (record, written.secret)
}
fn rotate(app: ApplicationId, client: ClientId, revision: u64, overlap_seconds: u16) -> Command {
    Command::RotateSecret {
        application: app,
        client,
        revision,
        overlap_seconds,
    }
}
async fn authenticates(f: &Fixture, client: ClientId, secret: &str) -> bool {
    f.db.store
        .authenticate_client(client, secret_digest(secret).unwrap())
        .await
        .is_ok()
}

#[tokio::test]
async fn registration_grants_rotation_retirement_and_current_metadata_round_trip() {
    let f = Fixture::new().await;
    let app = f.app().await;
    let Record::Resource(resource) = f
        .write(Command::CreateResource {
            application: app.id,
            name: Label::new("Orders API").unwrap(),
        })
        .await
        .record
    else {
        panic!("resource")
    };
    assert_eq!(
        resource.audience,
        format!(
            "urn:darkhorse:resource:{}",
            Uuid::from_u128(resource.id.as_u128())
        )
    );
    let Record::Scope(scope) = f
        .write(Command::CreateScope {
            application: app.id,
            resource: resource.id,
            name: ScopeName::new("orders.read").unwrap(),
        })
        .await
        .record
    else {
        panic!("scope")
    };
    let mut spec = client_spec();
    spec.resources.push(resource.id);
    spec.scopes.push(scope.id);
    let (client, first) = f.client(app.id, spec.clone()).await;
    assert_eq!(client.revision, 0);
    assert_eq!(client.spec.scopes, vec![scope.id]);
    assert!(
        client
            .spec
            .redirects
            .allows("https://app.example/callback?tenant=1")
    );
    assert!(
        !client
            .spec
            .redirects
            .allows("https://app.example/callback?tenant=2")
    );
    assert!(authenticates(&f, client.id, &first).await);
    assert!(!authenticates(&f, client.id, &"f".repeat(64)).await);
    assert!(!authenticates(&f, ClientId::from_u128(999).unwrap(), &first).await);
    let (rotated, second) = client_record(f.write(rotate(app.id, client.id, 0, 300)).await);
    let second = second.unwrap();
    assert_eq!(rotated.secrets.len(), 2);
    assert!(authenticates(&f, client.id, &first).await);
    assert!(authenticates(&f, client.id, &second).await);
    let (rotated, third) = client_record(f.write(rotate(app.id, client.id, 1, 0)).await);
    let third = third.unwrap();
    assert_eq!(rotated.secrets.len(), 1);
    assert!(!authenticates(&f, client.id, &first).await);
    assert!(!authenticates(&f, client.id, &second).await);
    assert!(authenticates(&f, client.id, &third).await);
    let raw_stored: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_client_secrets WHERE verifier=$1)")
            .bind(third.as_bytes())
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(!raw_stored);
    let (retired, secret) = client_record(
        f.write(Command::RetireSecret {
            application: app.id,
            client: client.id,
            secret: rotated.secrets[0].id,
            revision: 2,
        })
        .await,
    );
    assert!(secret.is_none());
    assert!(retired.secrets.is_empty());
    assert!(!authenticates(&f, client.id, &third).await);
    let (_, fourth) = client_record(f.write(rotate(app.id, client.id, 3, 0)).await);
    let fourth = fourth.unwrap();
    spec.scopes.clear();
    f.write(Command::UpdateClient {
        application: app.id,
        client: client.id,
        revision: 4,
        spec: spec.clone(),
    })
    .await;
    assert!(
        f.db.store
            .authenticate_client(client.id, secret_digest(&fourth).unwrap())
            .await
            .unwrap()
            .spec
            .scopes
            .is_empty()
    );
    spec.active = false;
    f.write(Command::UpdateClient {
        application: app.id,
        client: client.id,
        revision: 5,
        spec: spec.clone(),
    })
    .await;
    assert!(!authenticates(&f, client.id, &fourth).await);
    spec.active = true;
    f.write(Command::UpdateClient {
        application: app.id,
        client: client.id,
        revision: 6,
        spec,
    })
    .await;
    f.write(Command::UpdateApplication {
        application: app.id,
        revision: 0,
        spec: app_spec(false),
    })
    .await;
    assert!(!authenticates(&f, client.id, &fourth).await);
    f.write(Command::UpdateApplication {
        application: app.id,
        revision: 1,
        spec: app_spec(true),
    })
    .await;
    assert!(authenticates(&f, client.id, &fourth).await);
    sqlx::query(
        "UPDATE principals SET email='renamed@example.com',revision=revision+1 WHERE id=$1",
    )
    .bind(Uuid::from_u128(1))
    .execute(&f.db.pool)
    .await
    .unwrap();
    let Record::Application(view) = f
        .registry
        .read([1; 32], ReadTarget::Application(app.id))
        .await
        .unwrap()
    else {
        panic!("application")
    };
    assert_eq!(view.owner, id(1));
    assert_eq!(view.owner_email, "renamed@example.com");
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM registration_audit")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(events, 13);
}

#[tokio::test]
async fn cross_application_grants_and_stale_revisions_never_mutate_or_issue() {
    let f = Fixture::new().await;
    let a = f.app().await;
    let b = f.app().await;
    let (client, secret) = f.client(a.id, client_spec()).await;
    let Record::Resource(resource) = f
        .write(Command::CreateResource {
            application: b.id,
            name: Label::new("Other API").unwrap(),
        })
        .await
        .record
    else {
        panic!("resource")
    };
    let Record::Scope(scope) = f
        .write(Command::CreateScope {
            application: b.id,
            resource: resource.id,
            name: ScopeName::new("read").unwrap(),
        })
        .await
        .record
    else {
        panic!("scope")
    };
    for (resources, scopes) in [
        (vec![resource.id], vec![]),
        (vec![], vec![scope.id]),
        (vec![ResourceId::from_u128(999).unwrap()], vec![]),
        (vec![], vec![ScopeId::from_u128(999).unwrap()]),
    ] {
        let mut spec = client_spec();
        spec.resources = resources;
        spec.scopes = scopes;
        assert!(matches!(
            f.registry
                .write(
                    [1; 32],
                    Command::CreateClient {
                        application: a.id,
                        spec
                    }
                )
                .await,
            Err(RegistrationError::Invalid)
        ));
    }
    for command in [
        rotate(b.id, client.id, 0, 0),
        Command::UpdateClient {
            application: b.id,
            client: client.id,
            revision: 0,
            spec: client_spec(),
        },
        Command::CreateScope {
            application: a.id,
            resource: resource.id,
            name: ScopeName::new("read").unwrap(),
        },
        Command::RetireSecret {
            application: a.id,
            client: client.id,
            secret: ClientSecretId::from_u128(999).unwrap(),
            revision: 0,
        },
    ] {
        assert!(matches!(
            f.registry.write([1; 32], command).await,
            Err(RegistrationError::NotFound)
        ));
    }
    assert!(matches!(
        f.registry
            .write([1; 32], rotate(a.id, client.id, 99, 0))
            .await,
        Err(RegistrationError::Conflict)
    ));
    assert!(matches!(
        f.registry
            .write([1; 32], rotate(a.id, client.id, 0, 301))
            .await,
        Err(RegistrationError::Invalid)
    ));
    assert!(matches!(
        f.registry
            .read(
                [1; 32],
                ReadTarget::Client {
                    application: b.id,
                    client: client.id
                }
            )
            .await,
        Err(RegistrationError::NotFound)
    ));
    let mut spec = app_spec(true);
    spec.owner = id(999);
    assert!(matches!(
        f.registry
            .write([1; 32], Command::CreateApplication(spec))
            .await,
        Err(RegistrationError::Invalid)
    ));
    assert!(authenticates(&f, client.id, &secret).await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_client_secrets")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn concurrency_has_one_winner_and_final_authority_is_rechecked() {
    let f = Fixture::new().await;
    let app = f.app().await;
    let (client, original) = f.client(app.id, client_spec()).await;
    let (left, right) = tokio::join!(
        f.registry.write([1; 32], rotate(app.id, client.id, 0, 300)),
        f.registry.write([1; 32], rotate(app.id, client.id, 0, 300))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        matches!(left, Err(RegistrationError::Conflict))
            || matches!(right, Err(RegistrationError::Conflict))
    );
    assert!(authenticates(&f, client.id, &original).await);
    let command = rotate(app.id, client.id, 1, 0);
    f.db.store.preflight([1; 32], &command).await.unwrap();
    f.db.store
        .change(id(1), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let prepared = Prepared {
        identifier: None,
        secret: Some(OsRegistrationEntropy.secret().unwrap().verifier),
    };
    assert!(matches!(
        f.db.store.execute([1; 32], &command, prepared).await,
        Err(RegistrationError::Unauthorized)
    ));
    assert!(matches!(
        f.registry
            .read([1; 32], ReadTarget::Application(app.id))
            .await,
        Err(RegistrationError::Unauthorized)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_client_secrets")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn ownership_does_not_grant_administration_and_old_authentication_cannot_mutate() {
    let f = Fixture::new().await;
    insert_principal(&f.db, 2, false).await;
    insert_password(&f.db, 2).await;
    let candidate =
        f.db.store
            .candidate("person2@example.com")
            .await
            .unwrap()
            .unwrap();
    f.db.store
        .establish(&candidate, [2; 32], None)
        .await
        .unwrap();
    let mut spec = app_spec(true);
    spec.owner = id(2);
    let Record::Application(app) = f.write(Command::CreateApplication(spec)).await.record else {
        panic!("application")
    };
    assert!(matches!(
        f.registry
            .write([2; 32], Command::CreateApplication(app_spec(true)))
            .await,
        Err(RegistrationError::Forbidden)
    ));
    assert!(matches!(
        f.registry
            .read([2; 32], ReadTarget::Application(app.id))
            .await,
        Err(RegistrationError::Forbidden)
    ));
    assert!(matches!(
        f.registry
            .write([9; 32], Command::CreateApplication(app_spec(true)))
            .await,
        Err(RegistrationError::Unauthorized)
    ));
    sqlx::query("UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000 WHERE digest=$1").bind([1u8;32].as_slice()).execute(&f.db.pool).await.unwrap();
    assert!(
        f.registry
            .read([1; 32], ReadTarget::Application(app.id))
            .await
            .is_ok()
    );
    assert!(matches!(
        f.registry
            .write([1; 32], Command::CreateApplication(app_spec(true)))
            .await,
        Err(RegistrationError::RecentAuthenticationRequired)
    ));
}

#[tokio::test]
async fn audit_failure_and_unavailable_storage_return_no_secret_or_partial_change() {
    let f = Fixture::new().await;
    let app = f.app().await;
    let (client, secret) = f.client(app.id, client_spec()).await;
    let before: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_registration_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test failure'; END; $$; CREATE TRIGGER reject_audit BEFORE INSERT ON registration_audit FOR EACH ROW EXECUTE FUNCTION reject_registration_audit();").execute(&f.db.pool).await.unwrap();
    assert!(matches!(
        f.registry
            .write([1; 32], rotate(app.id, client.id, 0, 0))
            .await,
        Err(RegistrationError::Unavailable)
    ));
    assert!(authenticates(&f, client.id, &secret).await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_client_secrets")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        1
    );
    f.db.store.close().await;
    assert!(matches!(
        f.registry
            .write([1; 32], rotate(app.id, client.id, 0, 0))
            .await,
        Err(RegistrationError::Unavailable)
    ));
    assert!(matches!(
        f.registry
            .read([1; 32], ReadTarget::Application(app.id))
            .await,
        Err(RegistrationError::Unavailable)
    ));
    assert!(matches!(
        f.db.store
            .authenticate_client(client.id, secret_digest(&secret).unwrap())
            .await,
        Err(RegistrationError::Unavailable)
    ));
}
