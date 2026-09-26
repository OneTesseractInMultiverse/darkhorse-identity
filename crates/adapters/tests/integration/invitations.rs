use darkhorse_application::{
    bootstrap::PreparedCredential, email_delivery::DeliveryResult, invitations::*,
};
use darkhorse_domain::{
    directory::Profile,
    identity::{CredentialId, InvitationId, PrincipalId},
    invitations::Error,
};
fn material(n: u8) -> Material {
    Material {
        id: InvitationId::from_u128(n.into()).unwrap(),
        seed: [n; 32],
        digest: [n; 32],
    }
}
fn profile() -> Profile {
    Profile::new("new@example.com", "New", "Person").unwrap()
}
fn credential(n: u128) -> PreparedCredential {
    PreparedCredential {
        principal_id: PrincipalId::from_u128(n).unwrap(),
        credential_id: CredentialId::from_u128(n + 1000).unwrap(),
        verifier: super::administrator(1, "a@example.com").credential.verifier,
    }
}
#[tokio::test]
async fn onboarding_is_single_use_ordinary_verified_and_audited() {
    let db = super::oidc::fixture().await;
    assert_eq!(
        db.store
            .invite([0; 32], "new@example.com", material(3), None)
            .await,
        Err(Error::Unauthorized)
    );
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    assert_eq!(
        db.store
            .invite([1; 32], "new@example.com", material(4), None)
            .await,
        Err(Error::Throttled)
    );
    assert_eq!(
        db.store
            .invite([1; 32], "ONE@example.com", material(4), None)
            .await,
        Err(Error::Conflict)
    );
    assert_eq!(
        db.store
            .admit_invitation([3; 32], "wrong@example.com")
            .await,
        Err(Error::Invalid)
    );
    assert_eq!(
        db.store.admit_invitation([99; 32], "new@example.com").await,
        Err(Error::Invalid)
    );
    assert_eq!(
        db.store.admit_invitation([3; 32], "new@example.com").await,
        Ok(id)
    );
    let job = db.store.claim_invitation().await.unwrap().unwrap();
    assert_eq!(job.id, id);
    assert_eq!(job.attempt, 1);
    assert!(db.store.claim_invitation().await.unwrap().is_none());
    db.store
        .finish_invitation(id, 1, DeliveryResult::Accepted)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        db.store
            .accept_invitation(id, [3; 32], profile(), credential(100)),
        db.store
            .accept_invitation(id, [3; 32], profile(), credential(200))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        db.store.admit_invitation([3; 32], "new@example.com").await,
        Err(Error::Invalid)
    );
    let facts:(bool,bool,i64,i64)=sqlx::query_as("SELECT active,email_verified_ms IS NOT NULL,credential_epoch,revision FROM principals WHERE email_key='new@example.com'").fetch_one(&db.pool).await.unwrap();
    assert_eq!(facts, (true, true, 0, 0));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM platform_administrators")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM invitation_audit WHERE event='created'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert!(db.store.invitations([1; 32]).await.unwrap()[0].closed);
    assert!(db.store.claim_invitation().await.unwrap().is_none());
    db.store.close().await;
}
#[tokio::test]
async fn cancellation_after_admission_prevents_creation_and_attempts_survive_failures() {
    let db = super::oidc::fixture().await;
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    for _ in 0..5 {
        db.store
            .admit_invitation([3; 32], "new@example.com")
            .await
            .unwrap();
    }
    assert_eq!(
        db.store.admit_invitation([3; 32], "new@example.com").await,
        Err(Error::Throttled)
    );
    db.store.revoke_invitation([1; 32], id).await.unwrap();
    assert_eq!(
        db.store
            .accept_invitation(id, [3; 32], profile(), credential(100))
            .await,
        Err(Error::Invalid)
    );
    assert!(db.store.claim_invitation().await.unwrap().is_none());
    for query in [
        "UPDATE invitations SET closed=false",
        "UPDATE invitations SET expires_ms=expires_ms+1",
        "DELETE FROM invitations",
        "DELETE FROM invitation_audit",
        "UPDATE invitations SET hash_attempts=0",
    ] {
        assert!(sqlx::query(query).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
}
async fn extra_administrator(db: &super::Database) {
    let c = credential(500);
    sqlx::query("INSERT INTO principals(id,email,first_name,last_name) VALUES($1,'other@example.com','Other','Administrator')").bind(uuid::Uuid::from_u128(500)).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO credentials(id,principal_id,kind) VALUES($1,$2,'password')")
        .bind(uuid::Uuid::from_u128(501))
        .bind(uuid::Uuid::from_u128(500))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO password_credentials(credential_id,verifier) VALUES($1,$2)")
        .bind(uuid::Uuid::from_u128(501))
        .bind(c.verifier)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO platform_administrators VALUES($1)")
        .bind(uuid::Uuid::from_u128(500))
        .execute(&db.pool)
        .await
        .unwrap();
}
#[tokio::test]
async fn issuer_demotion_or_credential_revocation_after_hash_admission_is_terminal() {
    for action in [
        "DELETE FROM platform_administrators WHERE principal_id='00000000-0000-0000-0000-000000000001'",
        "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1 WHERE id='00000000-0000-0000-0000-000000000001'",
        "UPDATE credentials SET revoked=true WHERE principal_id='00000000-0000-0000-0000-000000000001'",
    ] {
        let db = super::oidc::fixture().await;
        extra_administrator(&db).await;
        let id = db
            .store
            .invite([1; 32], "new@example.com", material(3), None)
            .await
            .unwrap();
        db.store
            .admit_invitation([3; 32], "new@example.com")
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(action))
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        if action.starts_with("DELETE") {
            sqlx::query("INSERT INTO platform_administrators VALUES('00000000-0000-0000-0000-000000000001')").execute(&db.pool).await.unwrap();
        }
        assert_eq!(
            db.store
                .accept_invitation(id, [3; 32], profile(), credential(100))
                .await,
            Err(Error::Invalid)
        );
        assert!(db.store.claim_invitation().await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM principals WHERE email_key='new@example.com'"
            )
            .fetch_one(&db.pool)
            .await
            .unwrap(),
            0
        );
        db.store.close().await;
    }
}
#[tokio::test]
async fn audit_failure_rolls_back_new_account_consumption_and_issuance() {
    let db = super::oidc::fixture().await;
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    db.store
        .admit_invitation([3; 32], "new@example.com")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_invitation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE TRIGGER reject_invitation_audit BEFORE INSERT ON invitation_audit FOR EACH ROW EXECUTE FUNCTION reject_invitation_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store
            .accept_invitation(id, [3; 32], profile(), credential(100))
            .await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        db.store
            .invite([1; 32], "another@example.com", material(4), None)
            .await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM principals")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM invitations")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert!(!db.store.invitations([1; 32]).await.unwrap()[0].closed);
    sqlx::query("DROP TRIGGER reject_invitation_audit ON invitation_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .accept_invitation(id, [3; 32], profile(), credential(100))
        .await
        .unwrap();
    db.store.close().await;
}
#[tokio::test]
async fn concurrent_admission_has_a_shared_budget_and_no_hash_refund() {
    let db = super::oidc::fixture().await;
    db.store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    for _ in 0..4 {
        db.store
            .admit_invitation([3; 32], "new@example.com")
            .await
            .unwrap();
    }
    let (a, b) = tokio::join!(
        db.store.admit_invitation([3; 32], "new@example.com"),
        db.store.admit_invitation([3; 32], "new@example.com")
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM invitation_audit WHERE event='hash_admitted'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        5
    );
    db.store
        .invite([1; 32], "other@example.com", material(4), None)
        .await
        .unwrap();
    sqlx::query("INSERT INTO invitation_audit(invitation_id,event,attempt,occurred_ms) SELECT $1,'hash_admitted',1,floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM generate_series(1,55)").bind(uuid::Uuid::from_u128(3)).execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store
            .admit_invitation([4; 32], "other@example.com")
            .await,
        Err(Error::Throttled)
    );
    db.store.close().await;
}
#[tokio::test]
async fn stale_mail_acknowledgements_and_failures_cannot_reopen_revoked_proofs() {
    let db = super::oidc::fixture().await;
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    let job = db.store.claim_invitation().await.unwrap().unwrap();
    db.store
        .finish_invitation(id, 2, DeliveryResult::Accepted)
        .await
        .unwrap();
    db.store
        .finish_invitation(id, job.attempt, DeliveryResult::Retry)
        .await
        .unwrap();
    assert!(db.store.claim_invitation().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT delivery_state FROM invitations")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "queued"
    );
    db.store.revoke_invitation([1; 32], id).await.unwrap();
    db.store
        .finish_invitation(id, job.attempt, DeliveryResult::Accepted)
        .await
        .unwrap();
    assert_eq!(
        db.store.admit_invitation([3; 32], "new@example.com").await,
        Err(Error::Invalid)
    );
    db.store
        .invite([1; 32], "other@example.com", material(4), None)
        .await
        .unwrap();
    let job = db.store.claim_invitation().await.unwrap().unwrap();
    db.store
        .finish_invitation(job.id, 1, DeliveryResult::Rejected)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT delivery_state FROM invitations WHERE id=$1")
            .bind(uuid::Uuid::from_u128(4))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "failed"
    );
    db.store.close().await;
    assert_eq!(
        db.store.invitation_preflight([1; 32]).await,
        Err(Error::Unavailable)
    );
    assert!(db.store.claim_invitation().await.is_err());
    assert!(
        db.store
            .finish_invitation(id, 1, DeliveryResult::Retry)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn administrator_reads_and_mutations_require_current_role_and_recent_sign_in() {
    use darkhorse_application::authentication::AuthenticationStore;
    let db = super::oidc::fixture().await;
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    db.store
        .admit_invitation([3; 32], "new@example.com")
        .await
        .unwrap();
    db.store
        .accept_invitation(id, [3; 32], profile(), credential(100))
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("new@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [8; 32], None).await.unwrap();
    assert_eq!(
        db.store.invitation_preflight([8; 32]).await,
        Err(Error::Forbidden)
    );
    assert_eq!(db.store.invitations([8; 32]).await, Err(Error::Forbidden));
    assert_eq!(
        db.store.revoke_invitation([8; 32], id).await,
        Err(Error::Forbidden)
    );
    sqlx::query("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(repeat('09',32),'hex'),$1,principal_id,credential_id,credential_epoch,created_ms-300001,seen_ms,expires_ms-300001 FROM browser_sessions WHERE digest=decode(repeat('01',32),'hex')").bind(uuid::Uuid::from_u128(99)).execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.invitation_preflight([9; 32]).await,
        Err(Error::RecentAuthentication)
    );
    assert!(db.store.invitations([9; 32]).await.is_ok());
    assert_eq!(
        db.store.revoke_invitation([9; 32], id).await,
        Err(Error::RecentAuthentication)
    );
    db.store.close().await;
}
async fn dated_invitation(db: &super::Database, created_offset: i64) {
    sqlx::query("INSERT INTO invitations(id,email,issuer_id,issuer_credential_id,issuer_epoch,digest,seed,created_ms,expires_ms,next_ms) SELECT $1,'new@example.com',p.id,c.id,p.credential_epoch,decode(repeat('03',32),'hex'),decode(repeat('03',32),'hex'),t.ms+$2,t.ms+$2+86400000,t.ms+$2 FROM principals p JOIN credentials c ON c.principal_id=p.id CROSS JOIN (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS ms) t WHERE p.id='00000000-0000-0000-0000-000000000001'")
 .bind(uuid::Uuid::from_u128(3)).bind(created_offset).execute(&db.pool).await.unwrap();
}
#[tokio::test]
async fn expired_future_replaced_and_wrong_identity_proofs_never_create_accounts() {
    for offset in [-86_400_001, 60_000] {
        let db = super::oidc::fixture().await;
        dated_invitation(&db, offset).await;
        assert_eq!(
            db.store.admit_invitation([3; 32], "new@example.com").await,
            Err(Error::Invalid)
        );
        assert!(db.store.claim_invitation().await.unwrap().is_none());
        db.store.close().await;
    }
    let db = super::oidc::fixture().await;
    dated_invitation(&db, -900_001).await;
    let old = material(3).id;
    db.store
        .admit_invitation([3; 32], "new@example.com")
        .await
        .unwrap();
    assert_eq!(
        db.store
            .accept_invitation(material(99).id, [3; 32], profile(), credential(100))
            .await,
        Err(Error::Invalid)
    );
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(4), None)
        .await
        .unwrap();
    assert_eq!(
        db.store
            .accept_invitation(old, [3; 32], profile(), credential(100))
            .await,
        Err(Error::Invalid)
    );
    assert_eq!(
        db.store
            .accept_invitation(id, [4; 32], profile(), credential(100))
            .await,
        Err(Error::Invalid),
        "hashing must first obtain durable admission"
    );
    let job = db.store.claim_invitation().await.unwrap().unwrap();
    assert_eq!(job.id, id);
    db.store.revoke_invitation([1; 32], id).await.unwrap();
    db.store.revoke_invitation([1; 32], id).await.unwrap();
    db.store
        .revoke_invitation([1; 32], material(99).id)
        .await
        .unwrap();
    db.store.close().await;
}
#[tokio::test]
async fn acceptance_waiting_for_a_committed_revocation_fails_closed() {
    let db = super::oidc::fixture().await;
    let id = db
        .store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    db.store
        .admit_invitation([3; 32], "new@example.com")
        .await
        .unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let store = db.store.clone();
    let accepting = tokio::spawn(async move {
        store
            .accept_invitation(id, [3; 32], profile(), credential(100))
            .await
    });
    sqlx::query(
        "UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE id=$1",
    )
    .bind(uuid::Uuid::from_u128(3))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(accepting.await.unwrap(), Err(Error::Invalid));
    db.store.close().await;
}
#[tokio::test]
async fn issuance_counts_retained_requests_and_the_shared_mail_capacity() {
    use darkhorse_application::email_verification::{
        Material as VerificationMaterial, VerificationStore,
    };
    use darkhorse_domain::{
        email_verification::Error as VerificationError, identity::EmailVerificationId,
    };
    let db = super::oidc::fixture().await;
    extra_administrator(&db).await;
    // All data is disposable and synthetic: issued by the second administrator.
    sqlx::query("INSERT INTO invitations(id,email,issuer_id,issuer_credential_id,issuer_epoch,digest,seed,created_ms,expires_ms,next_ms) SELECT lpad(to_hex(n),32,'0')::uuid,'queued-'||n||'@example.com',$1,$2,0,decode(lpad(to_hex(n),64,'0'),'hex'),decode(repeat('02',32),'hex'),t.ms-900001,t.ms-900001+86400000,t.ms-900001 FROM generate_series(10000,19999) n CROSS JOIN (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS ms) t")
 .bind(uuid::Uuid::from_u128(500)).bind(uuid::Uuid::from_u128(501)).execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store
            .invite([1; 32], "new@example.com", material(3), None)
            .await,
        Err(Error::Unavailable)
    );
    assert_eq!(
        db.store
            .request_verification(
                [1; 32],
                VerificationMaterial {
                    id: EmailVerificationId::from_u128(4).unwrap(),
                    seed: [4; 32],
                    digest: [4; 32]
                }
            )
            .await,
        Err(VerificationError::Unavailable)
    );
    assert_eq!(db.store.invitations([1; 32]).await.unwrap().len(), 100);
    sqlx::query("UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled'")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .invite([1; 32], "new@example.com", material(3), None)
        .await
        .unwrap();
    use darkhorse_application::authentication::AuthenticationStore;
    let candidate = db
        .store
        .candidate("other@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [5; 32], None).await.unwrap();
    assert_eq!(
        db.store
            .invite([5; 32], "limit@example.com", material(4), None)
            .await,
        Err(Error::Throttled),
        "cancellation does not refund the issuing administrator budget"
    );
    db.store.close().await;
}
#[tokio::test]
async fn invitation_delivery_language_is_explicit_or_default_and_immutable() {
    use darkhorse_domain::localization::Locale;
    for (choice, fallback, expected) in [
        (None, Locale::Spanish, Locale::Spanish),
        (Some(Locale::English), Locale::Spanish, Locale::English),
        (Some(Locale::Spanish), Locale::English, Locale::Spanish),
    ] {
        let db = super::oidc::fixture().await;
        let store = db.store.clone().with_default_locale(fallback);
        store
            .invite([1; 32], "new@example.com", material(3), choice)
            .await
            .unwrap();
        let first = store.claim_invitation().await.unwrap().unwrap();
        assert_eq!(first.locale, expected);
        assert_eq!(first.template_version, 1);
        assert_eq!(first.expires_ms - first.created_ms, 86_400_000);
        store
            .finish_invitation(first.id, first.attempt, DeliveryResult::Retry)
            .await
            .unwrap();
        sqlx::query("UPDATE invitations SET next_ms=created_ms")
            .execute(&db.pool)
            .await
            .unwrap();
        let second = db.store.claim_invitation().await.unwrap().unwrap();
        assert_eq!(
            (
                second.locale,
                second.template_version,
                second.created_ms,
                second.expires_ms,
                second.seed
            ),
            (
                first.locale,
                first.template_version,
                first.created_ms,
                first.expires_ms,
                first.seed
            )
        );
        assert_eq!(second.attempt, 2);
        assert!(sqlx::query("UPDATE invitations SET delivery_locale=CASE WHEN delivery_locale='en' THEN 'es' ELSE 'en' END").execute(&db.pool).await.is_err());
        assert!(
            sqlx::query("UPDATE invitations SET template_version=0,delivery_locale='en'")
                .execute(&db.pool)
                .await
                .is_err()
        );
        db.pool.close().await;
    }
}
