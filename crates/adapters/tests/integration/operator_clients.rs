use super::operator_directory::{Allow, Held, Password};
use super::*;
use darkhorse_application::{
    operator_accounts::{self, Store},
    registration::{ClientRecord, Command, Prepared, Record, RegistrationStore},
};
use darkhorse_domain::{
    identity::{ApplicationId, ClientId, OperationId, ResourceId, ScopeId},
    operator_accounts::Error,
    operator_clients::{Request, Update},
    registration::{ClientSpec, Label, Redirects, RegistrationError},
};
fn update(revision: u64) -> Update {
    let mut spec = ClientSpec::new(
        Label::new("Private client name").unwrap(),
        true,
        Redirects::from_validated_urls(vec![
            "https://client.example/callback?fixed=1".into(),
            "https://client.example/new?exact=%2F".into(),
        ])
        .unwrap(),
        vec![ResourceId::from_u128(48).unwrap()],
        vec![ScopeId::from_u128(64).unwrap()],
        "client_secret_basic",
    )
    .unwrap();
    spec.refresh_tokens = true;
    Update {
        application: ApplicationId::from_u128(16).unwrap(),
        client: ClientId::from_u128(32).unwrap(),
        revision,
        spec,
    }
}
async fn fixture() -> Database {
    let db = oidc::fixture().await;
    resource_tokens::policy(&db).await;
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000060','00000000-0000-0000-0000-000000000020',$1,0)").bind([9u8;32].as_slice()).execute(&db.pool).await.unwrap();
    db
}
async fn write(
    store: &impl Store<Request = Request, Outcome = ClientRecord>,
    email: &str,
    update: Update,
) -> Result<ClientRecord, Error> {
    operator_accounts::run(
        store,
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        Request::new(update, "Approved client change").unwrap(),
        email,
        "source-only passphrase",
    )
    .await
}
async fn state(db: &Database) -> (String, i64) {
    let data:String=sqlx::query_scalar("SELECT jsonb_build_object('clients',(SELECT jsonb_agg(c ORDER BY id) FROM oauth_clients c),'redirects',(SELECT jsonb_agg(r ORDER BY client_id,uri) FROM client_redirects r),'resources',(SELECT jsonb_agg(r ORDER BY client_id,resource_id) FROM client_resources r),'scopes',(SELECT jsonb_agg(s ORDER BY client_id,scope_id) FROM client_scopes s),'secrets',(SELECT jsonb_agg(s ORDER BY id) FROM oauth_client_secrets s),'registration',(SELECT count(*) FROM registration_audit),'policy',(SELECT policy_revision FROM security_state))::text").fetch_one(&db.pool).await.unwrap();
    let audit = sqlx::query_scalar("SELECT count(*) FROM operator_client_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    (data, audit)
}
fn command(u: &Update) -> Command {
    Command::UpdateClient {
        application: u.application,
        client: u.client,
        revision: u.revision,
        spec: u.spec.clone(),
    }
}
fn prepared() -> Prepared {
    Prepared {
        identifier: None,
        secret: None,
    }
}
#[tokio::test]
async fn complete_updates_share_http_rules_preserve_secrets_and_commit_both_audits() {
    let db = fixture().await;
    let before: Vec<u8> = sqlx::query_scalar("SELECT verifier FROM oauth_client_secrets")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let u = update(0);
    let changed = write(&db.store.operator_clients(), "one@example.com", u.clone())
        .await
        .unwrap();
    assert_eq!(changed.revision, 1);
    assert_eq!(changed.spec, u.spec);
    assert!(changed.secrets.is_empty());
    let mut next = update(1);
    next.spec.resources.clear();
    next.spec.scopes.clear();
    next.spec.refresh_tokens = false;
    let Record::Client(changed) =
        RegistrationStore::execute(&db.store, [1; 32], &command(&next), prepared())
            .await
            .unwrap()
    else {
        panic!("client record")
    };
    assert_eq!(changed.spec, next.spec);
    assert_eq!(changed.revision, 2);
    let changed = write(&db.store.operator_clients(), "one@example.com", update(2))
        .await
        .unwrap();
    assert_eq!(changed.revision, 3);
    assert_eq!(
        before,
        sqlx::query_scalar::<_, Vec<u8>>("SELECT verifier FROM oauth_client_secrets")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_client_audit a")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(audits.len(), 2);
    assert!(audits.iter().all(|a| !a.contains("Private")
        && !a.contains("https://")
        && !a.contains("@example.com")
        && !a.contains("source-only")));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM registration_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        3
    );
}
#[tokio::test]
async fn scoped_targets_invalid_callbacks_allowances_and_owner_only_authority_cannot_write() {
    let db = fixture().await;
    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000050','Other','00000000-0000-0000-0000-000000000001',true); INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000051','00000000-0000-0000-0000-000000000050','Foreign API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000051'); INSERT INTO resource_scopes VALUES('00000000-0000-0000-0000-000000000052','00000000-0000-0000-0000-000000000050','00000000-0000-0000-0000-000000000051','foreign');").execute(&db.pool).await.unwrap();
    let mut foreign = update(0);
    foreign.application = ApplicationId::from_u128(80).unwrap();
    let mut missing = update(0);
    missing.client = ClientId::from_u128(999).unwrap();
    let mut resources = update(0);
    resources.spec.resources = vec![ResourceId::from_u128(81).unwrap()];
    resources.spec.scopes.clear();
    let mut scopes = update(0);
    scopes.spec.scopes = vec![ScopeId::from_u128(82).unwrap()];
    let mut unbound = update(0);
    unbound.spec.resources.clear();
    let mut callback = update(0);
    callback.spec.redirects =
        Redirects::from_validated_urls(vec!["http://client.example/cb".into()]).unwrap();
    for (email, u, error) in [
        ("person2@example.com", update(0), Error::Denied),
        ("missing@example.com", update(0), Error::Denied),
        ("one@example.com", foreign, Error::NotFound),
        ("one@example.com", missing, Error::NotFound),
        ("one@example.com", resources, Error::Invalid),
        ("one@example.com", scopes, Error::Invalid),
        ("one@example.com", unbound, Error::Invalid),
        ("one@example.com", callback, Error::Invalid),
        ("one@example.com", update(99), Error::Conflict),
    ] {
        let before = state(&db).await;
        assert!(matches!(write(&db.store.operator_clients(),email,u).await,Err(e) if e==error));
        let after = state(&db).await;
        assert_eq!(before.0, after.0);
        assert_eq!(after.1, before.1 + 1);
    }
}
#[tokio::test]
async fn concurrent_client_http_and_cli_updates_have_one_revision_winner() {
    let db = fixture().await;
    let u = update(0);
    let command = command(&u);
    let clients = db.store.operator_clients();
    let (a, b) = tokio::join!(
        write(&clients, "one@example.com", u),
        RegistrationStore::execute(&db.store, [1; 32], &command, prepared())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(a, Ok(_) | Err(Error::Conflict)));
    assert!(matches!(b, Ok(_) | Err(RegistrationError::Conflict)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn suppressed_or_failed_parent_bindings_and_audits_roll_back_the_entire_replacement() {
    for (table, event) in [
        ("oauth_clients", "UPDATE"),
        ("client_redirects", "INSERT"),
        ("client_redirects", "DELETE"),
        ("client_resources", "INSERT"),
        ("client_resources", "DELETE"),
        ("client_scopes", "INSERT"),
        ("client_scopes", "DELETE"),
        ("registration_audit", "INSERT"),
        ("operator_client_audit", "INSERT"),
    ] {
        for body in ["RETURN NULL;", "RAISE EXCEPTION 'fixture';"] {
            let db = fixture().await;
            let before = state(&db).await;
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION reject_client_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} END $$; CREATE TRIGGER reject_client_change BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION reject_client_change();"))).execute(&db.pool).await.unwrap();
            assert!(
                matches!(
                    write(&db.store.operator_clients(), "one@example.com", update(0)).await,
                    Err(Error::Unavailable)
                ),
                "{table} {event} {body}"
            );
            assert_eq!(state(&db).await, before);
            if table == "operator_client_audit" {
                assert!(matches!(
                    write(
                        &db.store.operator_clients(),
                        "missing@example.com",
                        update(0)
                    )
                    .await,
                    Err(Error::Unavailable)
                ));
            } else {
                assert!(
                    RegistrationStore::execute(
                        &db.store,
                        [1; 32],
                        &command(&update(0)),
                        prepared()
                    )
                    .await
                    .is_err()
                );
            }
            assert_eq!(state(&db).await, before);
        }
    }
}
#[tokio::test]
async fn client_writes_recheck_fence_waits_and_late_actor_reductions() {
    for age in [0, 60_000] {
        let db = fixture().await;
        insert_principal(&db, 3, true).await;
        let held = Held {
            inner: db.store.operator_clients(),
            ready: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
            age,
        };
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let reduce = async {
            held.ready.notified().await;
            held.resume.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
                let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%security_state%')").fetch_one(&db.pool).await.unwrap();
                if waiting {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }}).await.unwrap();
            if age == 0 {
                sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
                    .bind(Uuid::from_u128(1))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        };
        let (result, ()) = tokio::join!(write(&held, "one@example.com", update(0)), reduce);
        assert!(matches!(result, Err(Error::Denied)));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            0
        );
    }
    for (table, event) in [
        ("oauth_clients", "UPDATE"),
        ("operator_client_audit", "INSERT"),
    ] {
        let db = fixture().await;
        let before = state(&db).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION revoke_client_actor() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'; RETURN NEW; END $$; CREATE TRIGGER revoke_client_actor AFTER {event} ON {table} FOR EACH ROW EXECUTE FUNCTION revoke_client_actor();"))).execute(&db.pool).await.unwrap();
        assert!(matches!(
            write(&db.store.operator_clients(), "one@example.com", update(0)).await,
            Err(Error::Denied)
        ));
        let after = state(&db).await;
        assert_eq!(before.0, after.0);
        assert_eq!(
            after.1,
            before.1 + if table == "oauth_clients" { 1 } else { 0 }
        );
    }
}
#[tokio::test]
async fn failed_and_lost_client_commit_acknowledgements_never_repeat_the_update() {
    let db = fixture().await;
    let before = state(&db).await;
    sqlx::raw_sql("CREATE FUNCTION reject_client_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE CONSTRAINT TRIGGER reject_client_commit AFTER INSERT ON operator_client_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_client_commit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        write(&db.store.operator_clients(), "one@example.com", update(0)).await,
        Err(Error::Uncertain)
    ));
    assert_eq!(state(&db).await, before);
    sqlx::query("DROP TRIGGER reject_client_commit ON operator_client_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (store, proxy) = super::limiter_activation::lost_nth_commit(&db.pool, 1).await;
    assert!(matches!(
        write(&store.operator_clients(), "one@example.com", update(0)).await,
        Err(Error::Uncertain)
    ));
    proxy.await.unwrap();
    store.close().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM oauth_clients")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(state(&db).await.1, before.1 + 1);
}
#[tokio::test]
async fn committed_client_deactivation_and_allowance_reductions_reject_existing_resource_tokens() {
    use darkhorse_adapters::{
        resource_servers::OsResourceEntropy,
        tokens::material::{self, Purpose},
    };
    use darkhorse_application::{
        resource_servers::{self, Registry, ResourceTokenStore},
        tokens::{CodeStore, TokenStore},
    };
    for change in ["inactive", "resources", "scopes"] {
        let (db, signer) = tokens::setup().await;
        resource_tokens::policy(&db).await;
        let registry = resource_servers::Service {
            store: db.store.clone(),
            entropy: OsResourceEntropy,
        };
        let registered = registry
            .write(
                [1; 32],
                resource_servers::Command {
                    target: resource_servers::Target {
                        application: ApplicationId::from_u128(16).unwrap(),
                        resource: ResourceId::from_u128(48).unwrap(),
                    },
                    change: resource_servers::Change::Register,
                },
            )
            .await
            .unwrap();
        let verifier =
            darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap())
                .unwrap();
        resource_tokens::approve(&db, [3; 32]).await;
        let code = db
            .store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap(),
            )
            .await
            .unwrap();
        let token = db
            .store
            .redeem(
                tokens::input(&code),
                material::pair().unwrap(),
                tokens::ISSUER,
                &signer,
            )
            .await
            .unwrap();
        let digest = material::digest(&token.access, Purpose::Access).unwrap();
        let probe = || resource_servers::Probe {
            resource: ResourceId::from_u128(48).unwrap(),
            secret: verifier,
            token: Some(digest),
        };
        assert!(
            db.store
                .introspect_resource(probe(), tokens::ISSUER)
                .await
                .unwrap()
                .is_some()
        );
        let mut u = update(0);
        match change {
            "inactive" => u.spec.active = false,
            "resources" => {
                u.spec.resources.clear();
                u.spec.scopes.clear();
            }
            _ => u.spec.scopes.clear(),
        }
        write(&db.store.operator_clients(), "one@example.com", u)
            .await
            .unwrap();
        assert!(
            db.store
                .introspect_resource(probe(), tokens::ISSUER)
                .await
                .unwrap()
                .is_none()
        );
    }
}
#[tokio::test]
async fn migration_preserves_client_configuration_and_earlier_application_audit() {
    let db = Database::at_version(28).await;
    insert_principal(&db, 1, true).await;
    sqlx::raw_sql("INSERT INTO operator_application_audit(operation_id,command,owner_id,reason,result,occurred_ms) VALUES('00000000-0000-0000-0000-000000000002','application.create','00000000-0000-0000-0000-000000000001','Historical attempt','denied',1); INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','Historical app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000010','Historical client',true);").execute(&db.pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_application_audit a")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store
        .migrate_operation(OperationId::from_u128(3).unwrap())
        .await
        .unwrap();
    assert_eq!(
        before,
        sqlx::query_scalar::<_, String>(
            "SELECT row_to_json(a)::text FROM operator_application_audit a"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM oauth_clients")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "Historical client"
    );
    assert_eq!(state(&db).await.1, 0);
}
