use super::Database;
use darkhorse_application::signing::*;
use darkhorse_domain::signing::KeyError;
const ISSUER: &str = "https://issuer.example";
fn key(n: u8) -> WrappedKey {
    WrappedKey {
        public: PublicKey {
            kid: format!("{}{}", char::from(b'A' + n), "A".repeat(42)),
            n: "A".repeat(512),
            e: "AQAB".into(),
        },
        nonce: [n; 12],
        ciphertext: vec![n; 100],
    }
}
async fn bind(db: &Database) {
    db.store.bind_provider(ISSUER, [1; 32]).await.unwrap();
}
async fn elapsed(db: &Database, kid: &str) {
    // Only the disposable database owner advances lifecycle fixture times.
    sqlx::raw_sql("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE signing_keys SET created_ms=created_ms-60001 WHERE kid=$1")
        .bind(kid)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE signing_keys ENABLE TRIGGER signing_transition")
        .execute(&db.pool)
        .await
        .unwrap();
}
#[tokio::test]
async fn keys_are_pinned_prepublished_rotated_and_retired_without_private_projections() {
    let db = Database::new().await;
    bind(&db).await;
    assert_eq!(
        db.store
            .bind_provider("https://other.example", [1; 32])
            .await,
        Err(KeyError::Conflict)
    );
    assert_eq!(
        db.store.bind_provider(ISSUER, [2; 32]).await,
        Err(KeyError::Conflict)
    );
    assert_eq!(
        db.store.stage(ISSUER, [2; 32], 0, key(1)).await,
        Err(KeyError::Conflict)
    );
    let first = key(1).public.kid;
    assert_eq!(db.store.stage(ISSUER, [1; 32], 0, key(1)).await, Ok(1));
    assert_eq!(db.store.published(ISSUER).await.unwrap().len(), 1);
    assert_eq!(
        db.store.activate(ISSUER, &first, 1).await,
        Err(KeyError::NotReady)
    );
    elapsed(&db, &first).await;
    assert_eq!(db.store.activate(ISSUER, &first, 1).await, Ok(2));
    assert_eq!(
        db.store.retire(ISSUER, &first, 2).await,
        Err(KeyError::Conflict)
    );
    let second = key(2).public.kid;
    db.store.stage(ISSUER, [1; 32], 2, key(2)).await.unwrap();
    elapsed(&db, &second).await;
    assert_eq!(db.store.activate(ISSUER, &second, 3).await, Ok(4));
    assert_eq!(db.store.published(ISSUER).await.unwrap().len(), 2);
    assert_eq!(
        db.store.retire(ISSUER, &first, 4).await,
        Err(KeyError::NotReady)
    );
    sqlx::query("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition")
        .execute(&db.pool)
        .await
        .unwrap();
    // Advance only the old verification window while preserving the table's time constraints.
    sqlx::query("UPDATE signing_keys SET created_ms=created_ms-600001,activated_ms=activated_ms-600001,verify_until_ms=verify_until_ms-600001 WHERE kid=$1").bind(&first).execute(&db.pool).await.unwrap();
    sqlx::query("ALTER TABLE signing_keys ENABLE TRIGGER signing_transition")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(db.store.published(ISSUER).await.unwrap().len(), 1);
    assert_eq!(db.store.retire(ISSUER, &first, 4).await, Ok(5));
    let erased: bool =
        sqlx::query_scalar("SELECT ciphertext IS NULL FROM signing_keys WHERE kid=$1")
            .bind(&first)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(erased);
    assert_eq!(
        db.store.activate(ISSUER, &first, 5).await,
        Err(KeyError::Conflict)
    );
    assert_eq!(db.store.inventory(ISSUER).await.unwrap().keys.len(), 1);
}
#[tokio::test]
async fn key_races_capacity_duplicate_material_and_audit_failure_are_atomic() {
    let db = Database::new().await;
    bind(&db).await;
    let (a, b) = tokio::join!(
        db.store.stage(ISSUER, [1; 32], 0, key(1)),
        db.store.stage(ISSUER, [1; 32], 0, key(2))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(a == Err(KeyError::Conflict) || b == Err(KeyError::Conflict));
    let winner = if a.is_ok() { 1 } else { 2 };
    assert_eq!(
        db.store.stage(ISSUER, [1; 32], 1, key(winner)).await,
        Err(KeyError::Conflict)
    );
    for n in 3..6 {
        db.store
            .stage(ISSUER, [1; 32], u64::from(n - 2), key(n))
            .await
            .unwrap();
    }
    assert_eq!(
        db.store.stage(ISSUER, [1; 32], 4, key(6)).await,
        Err(KeyError::Conflict)
    );
    let kid = key(winner).public.kid;
    sqlx::raw_sql("CREATE FUNCTION reject_provider_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test failure'; END $$; CREATE TRIGGER reject_provider_audit BEFORE INSERT ON provider_audit FOR EACH ROW EXECUTE FUNCTION reject_provider_audit();").execute(&db.pool).await.unwrap();
    assert_eq!(
        db.store.retire(ISSUER, &kid, 4).await,
        Err(KeyError::Unavailable)
    );
    assert_eq!(db.store.inventory(ISSUER).await.unwrap().revision, 4);
    assert_eq!(db.store.published(ISSUER).await.unwrap().len(), 4);
    for sql in [
        "UPDATE provider_state SET issuer='https://other.example',revision=revision+1",
        "DELETE FROM signing_keys",
        "DELETE FROM provider_audit",
        "UPDATE signing_keys SET n=repeat('B',512)",
        "UPDATE signing_keys SET phase='active',activated_ms=created_ms+60000",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    db.store.close().await;
    assert_eq!(db.store.published(ISSUER).await, Err(KeyError::Unavailable));
}
