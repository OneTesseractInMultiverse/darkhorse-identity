use super::*;
use darkhorse_application::{authentication::AuthenticationStore, profiles::Store};
use darkhorse_domain::profiles::{Error, Input, Phone};
fn fields() -> darkhorse_domain::profiles::Fields {
    darkhorse_adapters::profiles::prepare(Input {
        first_name: "María".into(),
        second_name: "José".into(),
        last_name: "Guzmán".into(),
        second_last_name: "Benavides".into(),
        country: "CR".into(),
        bio: "🦀".repeat(2000),
        phone: Some(Phone::new("506", "88887777").unwrap()),
    })
    .unwrap()
}
async fn user(db: &Database) {
    insert_principal(db, 2, false).await;
    insert_password(db, 2).await;
    let candidate = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
}
#[tokio::test]
async fn saved_phone_values_remain_readable_and_correctable_after_metadata_updates() {
    let db = oidc::fixture().await;
    user(&db).await;
    // The previous validator accepted this extra digit. Restore an existing row;
    // new writes must still pass the current metadata before reaching the store.
    sqlx::query("UPDATE principals SET calling_code='353',national_number='22123450',revision=revision+1 WHERE id=$1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    let before = db.store.profile([2; 32], None).await.unwrap();
    assert_eq!(before.fields.phone().unwrap().e164(), "+35322123450");
    assert_eq!(
        db.store.profile([1; 32], Some(id(2))).await.unwrap().fields,
        before.fields
    );
    let after = db
        .store
        .update_profile([2; 32], None, before.revision, fields())
        .await
        .unwrap();
    assert_eq!(after.fields, fields());
    assert_eq!(after.revision, before.revision + 1);
    db.store.close().await;
}
#[tokio::test]
async fn profiles_are_owner_or_admin_only_audited_and_do_not_grant_authority() {
    let db = oidc::fixture().await;
    user(&db).await;
    assert!(matches!(
        db.store.profile([9; 32], None).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        db.store.profile([2; 32], Some(id(1))).await,
        Err(Error::Forbidden)
    ));
    let encoding:(i32,i32,String,String)=sqlx::query_as("SELECT length($1::text),octet_length($1::text),current_setting('server_encoding'),current_setting('client_encoding')").bind("🦀".repeat(2000)).fetch_one(&db.pool).await.unwrap();
    assert_eq!(encoding, (2000, 8000, "UTF8".into(), "UTF8".into()));
    let before = db.store.profile([2; 32], None).await.unwrap();
    assert_eq!(before.id, id(2));
    assert!(before.fields.phone().is_none());
    let after = db
        .store
        .update_profile([2; 32], None, before.revision, fields())
        .await
        .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.email, before.email);
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.fields, fields());
    assert!(!after.email_verified);
    assert_eq!(
        db.store.profile([1; 32], Some(id(2))).await.unwrap().fields,
        fields()
    );
    assert!(matches!(
        db.store
            .update_profile([2; 32], None, before.revision, fields())
            .await,
        Err(Error::Conflict)
    ));
    assert_eq!(
        db.store
            .update_profile([2; 32], None, after.revision, fields())
            .await
            .unwrap()
            .revision,
        after.revision
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM profile_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT credential_epoch FROM principals WHERE id=$1")
            .bind(Uuid::from_u128(2))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM platform_administrators WHERE principal_id=$1"
        )
        .bind(Uuid::from_u128(2))
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        sqlx::query("DELETE FROM profile_audit")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.logout([2; 32]).await.unwrap();
    assert!(matches!(
        db.store
            .update_profile([2; 32], None, after.revision, fields())
            .await,
        Err(Error::Unauthorized)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn profile_audit_failure_rolls_back_and_writes_require_recent_authentication() {
    let db = oidc::fixture().await;
    user(&db).await;
    sqlx::raw_sql("CREATE FUNCTION reject_profile_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test'; END $$; CREATE TRIGGER reject_profile BEFORE INSERT ON profile_audit FOR EACH ROW EXECUTE FUNCTION reject_profile_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store.update_profile([2; 32], None, 0, fields()).await,
        Err(Error::Unavailable)
    ));
    assert_eq!(db.store.profile([2; 32], None).await.unwrap().revision, 0);
    sqlx::raw_sql("DROP TRIGGER reject_profile ON profile_audit; ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000 WHERE digest=decode(repeat('02',32),'hex'); ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    assert!(db.store.profile([2; 32], None).await.is_ok());
    assert!(matches!(
        db.store.update_profile([2; 32], None, 0, fields()).await,
        Err(Error::RecentAuthentication)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn profile_concurrent_updates_have_one_winner_and_waiting_revocation_is_observed() {
    let db = oidc::fixture().await;
    user(&db).await;
    let (a, b) = tokio::join!(
        db.store.update_profile([2; 32], None, 0, fields()),
        db.store.update_profile([2; 32], None, 0, fields())
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(Error::Conflict)) | (Err(Error::Conflict), Ok(_))
    ));
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM security_state FOR UPDATE")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let store = db.store.clone();
    let pending = tokio::spawn(async move { store.profile([2; 32], None).await });
    sqlx::query("UPDATE principals SET active=false,credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=$1").bind(Uuid::from_u128(2)).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(pending.await.unwrap(), Err(Error::Unauthorized)));
    db.store.close().await;
}
#[tokio::test]
async fn language_preference_is_audited_without_changing_credentials_or_other_profile_fields() {
    use darkhorse_domain::localization::Locale;
    let db = oidc::fixture().await;
    user(&db).await;
    let before = db.store.profile([2; 32], None).await.unwrap();
    assert_eq!(before.locale, None);
    let session_before: (i64, i64, i64) = sqlx::query_as(
        "SELECT created_ms,expires_ms,credential_epoch FROM browser_sessions WHERE digest=$1",
    )
    .bind([2_u8; 32].as_slice())
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let saved = db
        .store
        .update_language([2; 32], before.revision, Some(Locale::Spanish))
        .await
        .unwrap();
    assert_eq!(saved.locale, Some(Locale::Spanish));
    assert_eq!(saved.fields, before.fields);
    assert_eq!(saved.revision, before.revision + 1);
    assert_eq!(
        db.store.session([2; 32]).await.unwrap().locale,
        Some(Locale::Spanish)
    );
    assert_eq!(
        db.store.profile([1; 32], Some(id(2))).await.unwrap().locale,
        Some(Locale::Spanish)
    );
    let same = db
        .store
        .update_language([2; 32], saved.revision, Some(Locale::Spanish))
        .await
        .unwrap();
    assert_eq!(same.revision, saved.revision);
    assert!(matches!(
        db.store
            .update_language([2; 32], before.revision, None)
            .await,
        Err(Error::Conflict)
    ));
    let edited = db
        .store
        .update_profile([2; 32], None, saved.revision, fields())
        .await
        .unwrap();
    assert_eq!(edited.locale, Some(Locale::Spanish));
    let cleared = db
        .store
        .update_language([2; 32], edited.revision, None)
        .await
        .unwrap();
    assert_eq!(cleared.locale, None);
    assert_eq!(cleared.fields, fields());
    let session_after: (i64, i64, i64) = sqlx::query_as(
        "SELECT created_ms,expires_ms,credential_epoch FROM browser_sessions WHERE digest=$1",
    )
    .bind([2_u8; 32].as_slice())
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(session_after, session_before);
    let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM profile_audit WHERE target_id=$1")
        .bind(Uuid::from_u128(2))
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(audit, 3);
    assert!(
        sqlx::query("UPDATE principals SET preferred_locale='fr' WHERE id=$1")
            .bind(Uuid::from_u128(2))
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}
#[tokio::test]
async fn language_writes_recheck_authority_and_roll_back_failed_audit() {
    use darkhorse_domain::localization::Locale;
    let db = oidc::fixture().await;
    user(&db).await;
    assert!(matches!(
        db.store
            .update_language([9; 32], 0, Some(Locale::Spanish))
            .await,
        Err(Error::Unauthorized)
    ));
    sqlx::raw_sql("CREATE FUNCTION reject_language_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture'; END $$; CREATE TRIGGER reject_language BEFORE INSERT ON profile_audit FOR EACH ROW EXECUTE FUNCTION reject_language_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .update_language([2; 32], 0, Some(Locale::Spanish))
            .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(db.store.profile([2; 32], None).await.unwrap().locale, None);
    sqlx::raw_sql("DROP TRIGGER reject_language ON profile_audit; CREATE FUNCTION revoke_at_language_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE principals SET active=false,credential_epoch=credential_epoch+1,revision=revision+1 WHERE id=NEW.actor_id; RETURN NEW; END $$; CREATE TRIGGER revoke_language AFTER INSERT ON profile_audit FOR EACH ROW EXECUTE FUNCTION revoke_at_language_audit();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .update_language([2; 32], 0, Some(Locale::Spanish))
            .await,
        Err(Error::Unauthorized)
    ));
    let current = db.store.profile([2; 32], None).await.unwrap();
    assert_eq!(current.locale, None);
    assert_eq!(current.revision, 0);
    sqlx::raw_sql("DROP TRIGGER revoke_language ON profile_audit;")
        .execute(&db.pool)
        .await
        .unwrap();
    let (one, two) = tokio::join!(
        db.store.update_language([2; 32], 0, Some(Locale::English)),
        db.store.update_language([2; 32], 0, Some(Locale::Spanish))
    );
    assert!(matches!(
        (&one, &two),
        (Ok(_), Err(Error::Conflict)) | (Err(Error::Conflict), Ok(_))
    ));
    sqlx::raw_sql("ALTER TABLE browser_sessions DISABLE TRIGGER browser_session_transition; UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000 WHERE digest=decode(repeat('02',32),'hex'); ALTER TABLE browser_sessions ENABLE TRIGGER browser_session_transition;").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store.update_language([2; 32], 1, None).await,
        Err(Error::RecentAuthentication)
    ));
    db.store.logout([2; 32]).await.unwrap();
    assert!(matches!(
        db.store.update_language([2; 32], 1, None).await,
        Err(Error::Unauthorized)
    ));
    db.store.close().await;
}

#[tokio::test]
async fn language_migration_preserves_existing_principal_data_and_unset_preferences() {
    let db = Database::at_version(31).await;
    insert_principal(&db, 2, false).await;
    sqlx::query(
        "UPDATE principals SET second_name='José',country='CR',bio='Existing profile',revision=revision+1 WHERE id=$1",
    )
    .bind(Uuid::from_u128(2))
    .execute(&db.pool)
    .await
    .unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM principals p WHERE id=$1")
            .bind(Uuid::from_u128(2))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store.migrate().await.unwrap();
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p)-'preferred_locale' FROM principals p WHERE id=$1")
            .bind(Uuid::from_u128(2))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT preferred_locale IS NULL FROM principals WHERE id=$1"
        )
        .bind(Uuid::from_u128(2))
        .fetch_one(&db.pool)
        .await
        .unwrap()
    );
    db.store.close().await;
}
