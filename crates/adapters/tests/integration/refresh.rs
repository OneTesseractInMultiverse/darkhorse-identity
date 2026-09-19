use super::tokens::{ISSUER, code, input, setup};
use super::*;
use darkhorse_adapters::tokens::{
    material::{self, Purpose},
    signer::Signer,
};
use darkhorse_application::resource_servers::Registry;
use darkhorse_application::{
    oidc::{AuthorizationStore, Decision},
    refresh::{RefreshMaintenance, RefreshStore, Request},
    tokens::{CodeStore, ManagedToken, Management, TokenManagementStore, TokenStore, Tokens},
};
use darkhorse_domain::tokens::Error;

async fn enabled() -> (Database, Signer) {
    let (db, signer) = setup().await;
    sqlx::query("UPDATE oauth_clients SET refresh_tokens=true,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    (db, signer)
}
fn refresh(token: &Tokens) -> Request {
    Request {
        client: super::oidc::request().client,
        secret: [9; 32],
        digest: material::digest(token.refresh.as_ref().unwrap(), Purpose::Refresh).unwrap(),
        scopes: None,
        resource: None,
    }
}
async fn issue(db: &Database, signer: &Signer, handle: [u8; 32], scopes: &[&str]) -> Tokens {
    let mut request = super::oidc::request();
    request.scopes = scopes.iter().map(|s| (*s).into()).collect();
    db.store
        .begin(request, handle, Some([1; 32]))
        .await
        .unwrap();
    db.store
        .resume(handle, Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    let code = db
        .store
        .issue(
            handle,
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    db.store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, signer)
        .await
        .unwrap()
}
async fn inactive(db: &Database, token: &Tokens) {
    assert!(matches!(
        db.store
            .userinfo(
                material::digest(&token.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await,
        Err(Error::InvalidToken)
    ));
}

#[tokio::test]
async fn refresh_concurrency_has_one_winner_and_replay_revokes_all_family_access() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    let other = issue(&db, &signer, [4; 32], &["openid"]).await;
    let (left, right) = tokio::join!(
        db.store
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER),
        db.store
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        matches!(&left, Err(Error::InvalidGrant)) || matches!(&right, Err(Error::InvalidGrant))
    );
    let winner = left.or(right).unwrap();
    assert!(winner.id_token.is_none());
    inactive(&db, &initial).await;
    inactive(&db, &winner).await;
    assert!(matches!(
        db.store
            .refresh(refresh(&winner), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        db.store
            .userinfo(
                material::digest(&other.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .is_ok()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM token_audit WHERE event='refresh_replayed'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    db.store.close().await;
}

#[tokio::test]
async fn refresh_narrows_disclosure_and_failed_expansion_does_not_consume_it() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid", "profile", "email"]).await;
    let mut request = refresh(&initial);
    request.scopes = Some(vec!["openid".into(), "email".into()]);
    let narrowed = db
        .store
        .refresh(request, material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    let profile = db
        .store
        .userinfo(
            material::digest(&narrowed.access, Purpose::Access).unwrap(),
            ISSUER,
        )
        .await
        .unwrap();
    assert!(profile.profile.is_none());
    assert!(profile.email.is_some());
    let mut expanded = refresh(&narrowed);
    expanded.scopes = Some(vec!["openid".into(), "profile".into()]);
    assert!(matches!(
        db.store
            .refresh(expanded, material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidScope)
    ));
    let next = db
        .store
        .refresh(refresh(&narrowed), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    assert_eq!(next.scope, "openid email");
    assert!(
        db.store
            .userinfo(
                material::digest(&initial.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .is_ok()
    );
    sqlx::query("UPDATE oauth_consents SET scopes=ARRAY['openid']")
        .execute(&db.pool)
        .await
        .unwrap();
    let mut reduced = refresh(&next);
    reduced.scopes = Some(vec!["openid".into()]);
    let reduced = db
        .store
        .refresh(reduced, material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    let view = db
        .store
        .userinfo(
            material::digest(&reduced.access, Purpose::Access).unwrap(),
            ISSUER,
        )
        .await
        .unwrap();
    assert!(view.email.is_none());
    assert!(view.profile.is_none());
    inactive(&db, &initial).await;
    db.store.close().await;
}

#[tokio::test]
async fn refresh_authentication_issuer_and_resource_substitution_do_not_revoke_the_owner() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    sqlx::raw_sql("INSERT INTO oauth_clients(id,application_id,name,active,refresh_tokens) VALUES('00000000-0000-0000-0000-000000000021','00000000-0000-0000-0000-000000000010','Other',true,true); INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000061','00000000-0000-0000-0000-000000000021',decode(repeat('08',32),'hex'),0);").execute(&db.pool).await.unwrap();
    let mut foreign = refresh(&initial);
    foreign.client = darkhorse_domain::identity::ClientId::from_u128(0x21).unwrap();
    foreign.secret = [8; 32];
    assert!(matches!(
        db.store
            .refresh(foreign, material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    for n in 0..3 {
        let mut request = refresh(&initial);
        match n {
            0 => request.secret = [0; 32],
            1 => request.client = darkhorse_domain::identity::ClientId::from_u128(99).unwrap(),
            _ => request.resource = Some("urn:foreign".into()),
        }
        assert!(
            db.store
                .refresh(request, material::pair().unwrap(), ISSUER)
                .await
                .is_err()
        );
    }
    assert!(matches!(
        db.store
            .refresh(
                refresh(&initial),
                material::pair().unwrap(),
                "https://foreign.example"
            )
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        db.store
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
            .await
            .is_ok()
    );
    db.store.close().await;
}

#[tokio::test]
async fn refresh_resource_permissions_only_shrink_across_reductions_and_later_growth() {
    use darkhorse_application::resource_servers::{
        Change, Command, Probe, ResourceTokenStore, Service, Target,
    };
    use darkhorse_domain::identity::{ApplicationId, CapabilityId, ResourceId};
    let (db, signer) = enabled().await;
    super::resource_tokens::policy(&db).await;
    let target = Target {
        application: ApplicationId::from_u128(0x10).unwrap(),
        resource: ResourceId::from_u128(0x30).unwrap(),
    };
    let registered = Service {
        store: db.store.clone(),
        entropy: darkhorse_adapters::resource_servers::OsResourceEntropy,
    }
    .write(
        [1; 32],
        Command {
            target,
            change: Change::Register,
        },
    )
    .await
    .unwrap();
    let verifier =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    super::resource_tokens::approve(&db, [3; 32]).await;
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let first = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let narrowed = db
        .store
        .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000071')").execute(&db.pool).await.unwrap();
    let next = db
        .store
        .refresh(refresh(&narrowed), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    let probe = |token: &Tokens| Probe {
        resource: target.resource,
        secret: verifier,
        token: Some(material::digest(&token.access, Purpose::Access).unwrap()),
    };
    let current = db
        .store
        .introspect_resource(probe(&next), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        current.capabilities,
        std::collections::BTreeSet::from([CapabilityId::from_u128(0x70).unwrap()])
    );
    assert!(matches!(
        db.store
            .userinfo(
                material::digest(&next.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await,
        Err(Error::InvalidToken)
    ));
    assert!(matches!(
        db.store
            .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        db.store
            .introspect_resource(probe(&next), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
}

#[tokio::test]
async fn refresh_live_state_changes_and_reactivation_never_resurrect_old_grants() {
    for statement in [
        "UPDATE oauth_clients SET refresh_tokens=false,revision=revision+1",
        "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1",
        "UPDATE browser_sessions SET revoked=true",
        "DELETE FROM oauth_consents",
        "UPDATE applications SET active=false,revision=revision+1",
    ] {
        let (db, signer) = enabled().await;
        let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
        sqlx::query(statement).execute(&db.pool).await.unwrap();
        assert!(
            db.store
                .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
                .await
                .is_err()
        );
        inactive(&db, &initial).await;
        sqlx::query("UPDATE oauth_clients SET refresh_tokens=true,revision=revision+1")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(
            db.store
                .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
                .await
                .is_err()
        );
        db.store.close().await;
    }
}

#[tokio::test]
async fn expired_refresh_denies_without_consumption_and_early_cleanup_is_forbidden() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    assert_eq!(db.store.prune_refresh().await.unwrap(), 0);
    assert!(
        sqlx::query("DELETE FROM refresh_families")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM refresh_tokens")
            .execute(&db.pool)
            .await
            .is_err()
    );
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    sqlx::raw_sql("ALTER TABLE refresh_tokens DISABLE TRIGGER refresh_member_transition; UPDATE refresh_tokens SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint-1; ALTER TABLE refresh_tokens ENABLE TRIGGER refresh_member_transition;").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM refresh_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}

#[tokio::test]
async fn refresh_audit_failure_rolls_back_and_lost_success_revokes_on_retry() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    sqlx::raw_sql("CREATE FUNCTION reject_refresh_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event='refresh_rotated' THEN RAISE EXCEPTION 'injected audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_refresh_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_refresh_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER reject_refresh_audit ON token_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let lost = db
        .store
        .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    // A new adapter instance has no in-memory replay state or cached response.
    let restarted = PostgresStore::from_pool(db.pool.clone());
    assert!(matches!(
        restarted
            .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    inactive(&db, &lost).await;
    db.store.close().await;
}

#[tokio::test]
async fn refresh_is_opt_in_and_code_replay_and_explicit_revocation_terminate_families() {
    let (db, signer) = setup().await;
    assert!(
        issue(&db, &signer, [3; 32], &["openid"])
            .await
            .refresh
            .is_none()
    );
    sqlx::query("UPDATE oauth_clients SET refresh_tokens=true,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    let authorization = code(&db, [4; 32]).await;
    let first = db
        .store
        .redeem(
            input(&authorization),
            material::pair().unwrap(),
            ISSUER,
            &signer,
        )
        .await
        .unwrap();
    let next = db
        .store
        .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .redeem(
                input(&authorization),
                material::pair().unwrap(),
                ISSUER,
                &signer
            )
            .await,
        Err(Error::InvalidGrant)
    ));
    inactive(&db, &next).await;
    assert!(
        db.store
            .refresh(refresh(&next), material::pair().unwrap(), ISSUER)
            .await
            .is_err()
    );
    let another = issue(&db, &signer, [5; 32], &["openid"]).await;
    db.store
        .revoke(
            Management {
                client: refresh(&another).client,
                secret: [9; 32],
                token: Some(ManagedToken::Refresh(refresh(&another).digest)),
            },
            ISSUER,
        )
        .await
        .unwrap();
    inactive(&db, &another).await;
    assert!(
        db.store
            .refresh(refresh(&another), material::pair().unwrap(), ISSUER)
            .await
            .is_err()
    );
    db.store.close().await;
}

async fn waiting(db: &Database, pattern: &str) {
    for _ in 0..200 {
        let found = sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)")
            .bind(pattern).fetch_one(&db.pool).await.unwrap();
        if found {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("operation did not reach the expected database lock");
}

#[tokio::test]
async fn an_expiring_client_secret_cannot_rotate_replay_or_revoke_after_waiting() {
    for action in 0..3 {
        let (db, signer) = enabled().await;
        let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
        if action == 1 {
            db.store
                .refresh(refresh(&initial), material::pair().unwrap(), ISSUER)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE oauth_client_secrets SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500").execute(&db.pool).await.unwrap();
        let mut blocker = db.pool.begin().await.unwrap();
        sqlx::query("SELECT digest FROM authorization_codes FOR UPDATE")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let store = db.store.clone();
        let request = refresh(&initial);
        let operation = tokio::spawn(async move {
            if action == 2 {
                store
                    .revoke(
                        Management {
                            client: request.client,
                            secret: request.secret,
                            token: Some(ManagedToken::Refresh(request.digest)),
                        },
                        ISSUER,
                    )
                    .await
            } else {
                store
                    .refresh(request, material::pair().unwrap(), ISSUER)
                    .await
                    .map(|_| ())
            }
        });
        waiting(&db, "SELECT * FROM authorization_codes%").await;
        sqlx::query("SELECT pg_sleep(GREATEST(0,(expires_ms-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM oauth_client_secrets").execute(&db.pool).await.unwrap();
        blocker.rollback().await.unwrap();
        assert_eq!(operation.await.unwrap(), Err(Error::InvalidClient));
        assert!(
            !sqlx::query_scalar::<_, bool>("SELECT revoked FROM refresh_families")
                .fetch_one(&db.pool)
                .await
                .unwrap()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_tokens")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            if action == 1 { 2 } else { 1 }
        );
        db.store.close().await;
    }
}

#[tokio::test]
async fn refresh_waiting_on_committing_revocation_uses_the_new_security_state() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    let mut writer = db.pool.begin().await.unwrap();
    sqlx::query("UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1")
        .execute(&mut *writer)
        .await
        .unwrap();
    let store = db.store.clone();
    let request = refresh(&initial);
    let operation = tokio::spawn(async move {
        store
            .refresh(request, material::pair().unwrap(), ISSUER)
            .await
    });
    waiting(&db, "SELECT singleton FROM security_state%").await;
    writer.commit().await.unwrap();
    assert!(matches!(operation.await.unwrap(), Err(Error::InvalidGrant)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}

#[tokio::test]
async fn families_stop_at_256_members_and_database_guards_reject_expansion() {
    let (db, signer) = enabled().await;
    let mut token = issue(&db, &signer, [3; 32], &["openid", "email"]).await;
    let root = refresh(&token).digest;
    for statement in [
        "UPDATE refresh_families SET issuer='https://foreign.example'",
        "UPDATE refresh_tokens SET generation=1",
        "INSERT INTO refresh_tokens SELECT decode(repeat('ff',32),'hex'),code_digest,2,scope,capability_ceiling,created_ms,expires_ms,false FROM refresh_tokens",
        "UPDATE access_tokens SET refresh_generation=NULL",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    for _ in 0..255 {
        token = db
            .store
            .refresh(refresh(&token), material::pair().unwrap(), ISSUER)
            .await
            .unwrap();
    }
    assert!(matches!(
        db.store
            .refresh(refresh(&token), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        256
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        256
    );
    assert!(
        sqlx::query("UPDATE refresh_tokens SET consumed=false WHERE digest=$1")
            .bind(root.as_slice())
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}

// Controlled database-owner fixture: move only family/member/access history back in time.
// Trigger bypass is restricted to this disposable test fixture.
async fn age_history(db: &Database) {
    sqlx::raw_sql("ALTER TABLE refresh_families DISABLE TRIGGER refresh_family_transition; ALTER TABLE refresh_tokens DISABLE TRIGGER refresh_member_transition; ALTER TABLE access_tokens DISABLE TRIGGER access_transition; UPDATE refresh_families SET created_ms=created_ms-172800000,expires_ms=expires_ms-172800000; UPDATE refresh_tokens SET created_ms=created_ms-172800000,expires_ms=expires_ms-172800000; UPDATE access_tokens SET created_ms=created_ms-172800000,expires_ms=expires_ms-172800000; ALTER TABLE refresh_families ENABLE TRIGGER refresh_family_transition; ALTER TABLE refresh_tokens ENABLE TRIGGER refresh_member_transition; ALTER TABLE access_tokens ENABLE TRIGGER access_transition;").execute(&db.pool).await.unwrap();
}

#[tokio::test]
async fn cleanup_is_bounded_skips_busy_families_and_preserves_audit_and_live_grants() {
    let (db, signer) = enabled().await;
    for handle in 3..25 {
        issue(&db, &signer, [handle; 32], &["openid"]).await;
    }
    age_history(&db).await;
    let live = issue(&db, &signer, [25; 32], &["openid"]).await;
    let before = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM token_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT c.digest FROM authorization_codes c JOIN refresh_families f ON f.code_digest=c.digest ORDER BY f.expires_ms,f.code_digest LIMIT 1 FOR UPDATE OF c").execute(&mut *blocker).await.unwrap();
    let (left, right) = tokio::join!(db.store.prune_refresh(), db.store.prune_refresh());
    assert_eq!(left.unwrap(), 10);
    assert_eq!(right.unwrap(), 10);
    assert_eq!(db.store.prune_refresh().await.unwrap(), 1);
    assert_eq!(db.store.prune_refresh().await.unwrap(), 0);
    blocker.rollback().await.unwrap();
    assert_eq!(db.store.prune_refresh().await.unwrap(), 1);
    for table in ["refresh_families", "refresh_tokens", "access_tokens"] {
        let query = format!("SELECT count(*) FROM {table}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            1
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM token_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        before
    );
    db.store
        .refresh(refresh(&live), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    db.store.close().await;
}

#[tokio::test]
async fn a_refresh_member_expiring_while_waiting_cannot_issue_credentials() {
    let (db, signer) = enabled().await;
    let initial = issue(&db, &signer, [3; 32], &["openid"]).await;
    sqlx::raw_sql("ALTER TABLE refresh_tokens DISABLE TRIGGER refresh_member_transition; UPDATE refresh_tokens SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500; ALTER TABLE refresh_tokens ENABLE TRIGGER refresh_member_transition;").execute(&db.pool).await.unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT digest FROM authorization_codes FOR UPDATE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let request = refresh(&initial);
    let operation = tokio::spawn(async move {
        store
            .refresh(request, material::pair().unwrap(), ISSUER)
            .await
    });
    waiting(&db, "SELECT * FROM authorization_codes%").await;
    sqlx::query("SELECT pg_sleep(GREATEST(0,(expires_ms-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM refresh_tokens").execute(&db.pool).await.unwrap();
    blocker.rollback().await.unwrap();
    assert!(matches!(operation.await.unwrap(), Err(Error::InvalidGrant)));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM refresh_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}

#[tokio::test]
async fn replay_audit_failure_rolls_back_revocation_and_expired_family_cannot_rotate() {
    let (db, signer) = enabled().await;
    let first = issue(&db, &signer, [3; 32], &["openid"]).await;
    let next = db
        .store
        .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_replay_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event='refresh_replayed' THEN RAISE EXCEPTION 'injected audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_replay_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_replay_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::Unavailable)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT revoked FROM refresh_families")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert!(
        db.store
            .userinfo(
                material::digest(&next.access, Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .is_ok()
    );
    age_history(&db).await;
    for token in [&first, &next] {
        assert!(matches!(
            db.store
                .refresh(refresh(token), material::pair().unwrap(), ISSUER)
                .await,
            Err(Error::InvalidGrant)
        ));
    }
    db.store.close().await;
}

#[tokio::test]
async fn refresh_subject_binding_and_replay_are_isolated_between_users_of_one_client() {
    use darkhorse_application::authentication::AuthenticationStore;
    let (db, signer) = enabled().await;
    let first = issue(&db, &signer, [3; 32], &["openid"]).await;
    super::insert_principal(&db, 2, false).await;
    super::insert_password(&db, 2).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    db.store
        .begin(super::oidc::request(), [4; 32], Some([2; 32]))
        .await
        .unwrap();
    db.store
        .resume([4; 32], Some([2; 32]), Decision::Approve)
        .await
        .unwrap();
    let code = db
        .store
        .issue(
            [4; 32],
            Some([2; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let second = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    assert!(sqlx::query("UPDATE authorization_codes SET principal_id='00000000-0000-0000-0000-000000000001' WHERE digest=$1").bind(material::digest(&code.value,Purpose::Code).unwrap().as_slice()).execute(&db.pool).await.is_err());
    assert!(
        sqlx::query("UPDATE refresh_tokens SET code_digest=$1 WHERE digest=$2")
            .bind(
                material::digest(&code.value, Purpose::Code)
                    .unwrap()
                    .as_slice()
            )
            .bind(refresh(&first).digest.as_slice())
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store
        .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .refresh(refresh(&first), material::pair().unwrap(), ISSUER)
            .await,
        Err(Error::InvalidGrant)
    ));
    let second = db
        .store
        .refresh(refresh(&second), material::pair().unwrap(), ISSUER)
        .await
        .unwrap();
    let view = db
        .store
        .userinfo(
            material::digest(&second.access, Purpose::Access).unwrap(),
            ISSUER,
        )
        .await
        .unwrap();
    assert_eq!(view.subject, super::id(2));
    db.store.close().await;
}

#[tokio::test]
async fn initial_family_issuance_rechecks_client_expiry_after_signing() {
    use darkhorse_application::{
        signing::WrappedKey,
        tokens::{IdClaims, IdSigner},
    };
    struct Delayed {
        signer: Signer,
        pool: PgPool,
    }
    impl IdSigner for Delayed {
        async fn sign(&self, key: WrappedKey, claims: IdClaims) -> Result<String, Error> {
            let value = self.signer.sign(key, claims).await?;
            sqlx::query("SELECT pg_sleep(GREATEST(0,(expires_ms-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM oauth_client_secrets").execute(&self.pool).await.unwrap();
            Ok(value)
        }
    }
    let (db, signer) = enabled().await;
    let authorization = code(&db, [3; 32]).await;
    sqlx::query("UPDATE oauth_client_secrets SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .redeem(
                input(&authorization),
                material::pair().unwrap(),
                ISSUER,
                &Delayed {
                    signer,
                    pool: db.pool.clone()
                }
            )
            .await,
        Err(Error::InvalidClient)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM refresh_families")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}
