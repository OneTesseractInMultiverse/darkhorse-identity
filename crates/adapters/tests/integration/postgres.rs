use darkhorse_adapters::postgres::PostgresStore;
use darkhorse_application::{
    bootstrap::{BootstrapError, BootstrapStore, NewAdministrator, PreparedCredential},
    directory::{DirectoryFailure, DirectoryStore},
};
use darkhorse_domain::{
    AccountStatus,
    directory::{AccountAction, DirectoryError, Profile},
    identity::{CredentialId, PrincipalId},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
mod admin_directory;
mod authentication;
mod operator_directory;
mod personal_keys;
mod refresh;
mod registration;
mod relying_party_sessions;
mod sessions;
mod signing;

struct Database {
    pool: PgPool,
    store: PostgresStore,
}
impl Database {
    async fn new() -> Self {
        Self::at_version(i64::MAX).await
    }
    async fn at_version(version: i64) -> Self {
        let url = std::env::var("DARKHORSE_TEST_DATABASE_URL")
            .expect("use make test-postgres for disposable infrastructure");
        let admin = PgPool::connect(&url).await.unwrap();
        let name = format!("test_{}", Uuid::new_v4().simple());
        // Identifier contains only a fixed prefix and hexadecimal UUID digits.
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&admin)
            .await
            .unwrap();
        let mut url = url::Url::parse(&url).unwrap();
        url.set_path(&name);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(url.as_str())
            .await
            .unwrap();
        admin.close().await;
        let store = PostgresStore::from_pool(pool.clone());
        let mut migrations = sqlx::migrate!("./migrations");
        migrations.migrations = migrations
            .iter()
            .filter(|m| m.version <= version)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        migrations.run(&pool).await.unwrap();
        Self { pool, store }
    }
}
fn id(n: u128) -> PrincipalId {
    PrincipalId::from_u128(n).unwrap()
}
fn administrator(n: u128, email: &str) -> NewAdministrator {
    NewAdministrator { profile: Profile::new(email,"Ada","Lovelace").unwrap(), credential: PreparedCredential { principal_id: id(n), credential_id: CredentialId::from_u128(n+100).unwrap(), verifier: "$argon2id$v=19$m=65536,t=3,p=1$c29tZXNhbHRoZXJlMTIzNA$testfixtureonlynotusedforlogin".to_owned() } }
}
async fn count(db: &Database, table: &str) -> i64 {
    let query = match table {
        "principals" => "SELECT count(*) FROM principals",
        "credentials" => "SELECT count(*) FROM credentials",
        "password_credentials" => "SELECT count(*) FROM password_credentials",
        "platform_administrators" => "SELECT count(*) FROM platform_administrators",
        "security_audit" => "SELECT count(*) FROM security_audit",
        _ => panic!("unknown test table"),
    };
    sqlx::query_scalar(query).fetch_one(&db.pool).await.unwrap()
}

#[tokio::test]
async fn migrations_are_repeatable_and_concurrent_bootstrap_has_one_winner() {
    let db = Database::new().await;
    db.store.migrate().await.unwrap();
    assert_eq!(count(&db, "principals").await, 0);
    assert!(
        sqlx::query("UPDATE security_state SET bootstrapped=true")
            .execute(&db.pool)
            .await
            .is_err()
    );
    let (left, right) = tokio::join!(
        db.store.bootstrap(administrator(1, "one@example.com")),
        db.store.bootstrap(administrator(2, "two@example.com"))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        left == Err(BootstrapError::AlreadyInitialized)
            || right == Err(BootstrapError::AlreadyInitialized)
    );
    for table in [
        "principals",
        "credentials",
        "password_credentials",
        "platform_administrators",
        "security_audit",
    ] {
        assert_eq!(count(&db, table).await, 1);
    }
    let winner = left.or(right).unwrap();
    let account = db.store.account(winner).await.unwrap();
    assert!(account.administrator);
    assert_eq!(account.credential_epoch, 0);
    assert_eq!(
        db.store
            .bootstrap(administrator(3, "three@example.com"))
            .await,
        Err(BootstrapError::AlreadyInitialized)
    );
    assert_eq!(
        db.store
            .change(winner, 0, AccountAction::SetStatus(AccountStatus::Inactive))
            .await,
        Err(DirectoryFailure::Policy(DirectoryError::LastAdministrator))
    );
    db.store.close().await;
}

async fn insert_principal(db: &Database, n: u128, administrator: bool) {
    let identifier = Uuid::from_u128(n);
    sqlx::query(
        "INSERT INTO principals (id,email,first_name,last_name) VALUES ($1,$2,'Test','Person')",
    )
    .bind(identifier)
    .bind(format!("person{n}@example.com"))
    .execute(&db.pool)
    .await
    .unwrap();
    if administrator {
        sqlx::query("INSERT INTO platform_administrators (principal_id) VALUES ($1)")
            .bind(identifier)
            .execute(&db.pool)
            .await
            .unwrap();
        insert_password(db, n).await;
    }
}

async fn insert_password(db: &Database, n: u128) {
    sqlx::query("INSERT INTO credentials (id,principal_id,kind) VALUES ($1,$2,'password')")
        .bind(Uuid::from_u128(n + 100))
        .bind(Uuid::from_u128(n))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO password_credentials (credential_id,verifier) VALUES ($1,$2)")
        .bind(Uuid::from_u128(n + 100))
        .bind(administrator(n, "fixture@example.com").credential.verifier)
        .execute(&db.pool)
        .await
        .unwrap();
}

async fn reject_audit(db: &Database) {
    sqlx::raw_sql("CREATE FUNCTION reject_test_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END; $$; CREATE TRIGGER injected_audit_failure BEFORE INSERT ON security_audit FOR EACH ROW EXECUTE FUNCTION reject_test_audit();").execute(&db.pool).await.unwrap();
}

#[tokio::test]
async fn audit_failure_rolls_back_bootstrap_and_account_changes() {
    let db = Database::new().await;
    reject_audit(&db).await;
    assert_eq!(
        db.store
            .bootstrap(administrator(1, "one@example.com"))
            .await,
        Err(BootstrapError::Storage)
    );
    for table in [
        "principals",
        "credentials",
        "password_credentials",
        "platform_administrators",
        "security_audit",
    ] {
        assert_eq!(count(&db, table).await, 0);
    }
    let state: (bool, i64) =
        sqlx::query_as("SELECT bootstrapped,policy_revision FROM security_state")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(state, (false, 0));
    sqlx::query("DROP TRIGGER injected_audit_failure ON security_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    reject_audit_after_creation(&db).await;
    assert_eq!(
        db.store.change(id(1), 0, AccountAction::RevokeAll).await,
        Err(DirectoryFailure::Unavailable)
    );
    assert_eq!(db.store.account(id(1)).await.unwrap().credential_epoch, 0);
    assert_eq!(count(&db, "security_audit").await, 1);
    db.store.close().await;
}

async fn reject_audit_after_creation(db: &Database) {
    sqlx::query("CREATE TRIGGER injected_audit_failure BEFORE INSERT ON security_audit FOR EACH ROW EXECUTE FUNCTION reject_test_audit()").execute(&db.pool).await.unwrap();
}

#[tokio::test]
async fn directory_transitions_preserve_revocation_and_optimistic_versions() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    insert_principal(&db, 2, false).await;
    assert_eq!(
        db.store.account(id(999)).await,
        Err(DirectoryFailure::NotFound)
    );
    assert_eq!(
        db.store.change(id(999), 0, AccountAction::RevokeAll).await,
        Err(DirectoryFailure::NotFound)
    );
    assert_eq!(
        db.store
            .change(id(2), 0, AccountAction::SetStatus(AccountStatus::Active))
            .await,
        Ok(None)
    );
    let inactive = db
        .store
        .change(id(2), 0, AccountAction::SetStatus(AccountStatus::Inactive))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inactive.credential_epoch, 1);
    assert_eq!(
        db.store.account(id(2)).await.unwrap().status,
        AccountStatus::Inactive
    );
    assert_eq!(
        db.store.change(id(2), 0, AccountAction::RevokeAll).await,
        Err(DirectoryFailure::Conflict)
    );
    let active = db
        .store
        .change(id(2), 1, AccountAction::SetStatus(AccountStatus::Active))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.credential_epoch, 1);
    let (left, right) = tokio::join!(
        db.store.change(id(2), 2, AccountAction::RevokeAll),
        db.store.change(id(2), 2, AccountAction::RevokeAll)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(left == Err(DirectoryFailure::Conflict) || right == Err(DirectoryFailure::Conflict));
    let record = db.store.account(id(2)).await.unwrap();
    assert_eq!((record.credential_epoch, record.revision), (2, 3));
    assert_eq!(count(&db, "security_audit").await, 4);
    db.store.close().await;
}

#[tokio::test]
async fn concurrent_administrator_deactivation_preserves_one_active_administrator() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    insert_principal(&db, 2, true).await;
    let (left, right) = tokio::join!(
        db.store
            .change(id(1), 0, AccountAction::SetStatus(AccountStatus::Inactive)),
        db.store
            .change(id(2), 0, AccountAction::SetStatus(AccountStatus::Inactive))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        left == Err(DirectoryFailure::Policy(DirectoryError::LastAdministrator))
            || right == Err(DirectoryFailure::Policy(DirectoryError::LastAdministrator))
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM principals p JOIN platform_administrators a ON a.principal_id=p.id WHERE p.active").fetch_one(&db.pool).await.unwrap();
    assert_eq!(remaining, 1);
    assert!(
        sqlx::query("DELETE FROM platform_administrators")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}

#[tokio::test]
async fn database_constraints_retain_identity_and_reject_inconsistent_security_state() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "Mixed@Example.COM"))
        .await
        .unwrap();
    let duplicate = sqlx::query("INSERT INTO principals (id,email,first_name,last_name) VALUES ($1,'mixed@example.com','Other','User')").bind(Uuid::from_u128(2)).execute(&db.pool).await.unwrap_err();
    assert_eq!(
        duplicate.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    for query in [
        "UPDATE principals SET id='00000000-0000-0000-0000-000000000099',revision=revision+1",
        "DELETE FROM principals",
        "UPDATE principals SET active=false,revision=revision+1",
        "UPDATE principals SET credential_epoch=-1,revision=revision+1",
        "UPDATE principals SET revision=-1",
        "UPDATE security_state SET bootstrapped=false",
        "DELETE FROM security_state",
        "UPDATE security_audit SET event='account.revoked'",
        "DELETE FROM security_audit",
        "DELETE FROM credentials",
        "UPDATE credentials SET kind='changed'",
        "INSERT INTO platform_administrators (principal_id) VALUES ('00000000-0000-0000-0000-000000000099')",
    ] {
        assert!(
            sqlx::query(query).execute(&db.pool).await.is_err(),
            "{query}"
        );
    }
    assert!(
        sqlx::query("UPDATE credentials SET revoked=true")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM password_credentials")
            .execute(&db.pool)
            .await
            .is_err()
    );
    insert_principal(&db, 2, true).await;
    sqlx::query("UPDATE credentials SET revoked=true WHERE principal_id=$1")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE credentials SET revoked=false")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}

#[tokio::test]
async fn membership_without_a_password_cannot_allow_removing_the_last_eligible_administrator() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    insert_principal(&db, 2, false).await;
    sqlx::query("INSERT INTO platform_administrators (principal_id) VALUES ($1)")
        .bind(Uuid::from_u128(2))
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store
            .change(id(1), 0, AccountAction::SetStatus(AccountStatus::Inactive))
            .await,
        Err(DirectoryFailure::Policy(DirectoryError::LastAdministrator))
    );
    assert!(
        db.store
            .change(id(2), 0, AccountAction::SetStatus(AccountStatus::Inactive))
            .await
            .is_ok()
    );
    db.store.close().await;
}

#[tokio::test]
async fn concurrent_credential_revocation_preserves_an_eligible_administrator() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    insert_principal(&db, 2, true).await;
    let (left, right) = tokio::join!(
        sqlx::query("UPDATE credentials SET revoked=true WHERE principal_id=$1")
            .bind(Uuid::from_u128(1))
            .execute(&db.pool),
        sqlx::query("UPDATE credentials SET revoked=true WHERE principal_id=$1")
            .bind(Uuid::from_u128(2))
            .execute(&db.pool)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let error = left.err().or(right.err()).unwrap();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM eligible_administrators")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 1);
    db.store.close().await;
}

#[tokio::test]
async fn capability_identity_and_revision_dependencies_are_persistent() {
    let db = Database::new().await;
    sqlx::query("INSERT INTO capabilities (id,permission_key,meaning) VALUES ($1,'directory.read','Read the user directory')").bind(Uuid::from_u128(5)).execute(&db.pool).await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(before, 1);
    for query in [
        "UPDATE capabilities SET meaning='Delete users'",
        "UPDATE capabilities SET permission_key='directory.delete'",
        "DELETE FROM capabilities",
    ] {
        assert!(sqlx::query(query).execute(&db.pool).await.is_err());
    }
    sqlx::query("UPDATE capabilities SET retired=true")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE capabilities SET retired=false")
            .execute(&db.pool)
            .await
            .is_err()
    );
    let after: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(after, 2);
    db.store.close().await;
}

#[tokio::test]
async fn overflow_and_unavailable_storage_never_partially_change_authority() {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    sqlx::query("UPDATE principals SET credential_epoch=9223372036854775807,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.change(id(1), 1, AccountAction::RevokeAll).await,
        Err(DirectoryFailure::Policy(DirectoryError::CounterExhausted))
    );
    assert_eq!(count(&db, "security_audit").await, 1);
    db.store.close().await;
    assert_eq!(
        db.store.account(id(1)).await,
        Err(DirectoryFailure::Unavailable)
    );
    assert_eq!(
        db.store.change(id(1), 1, AccountAction::RevokeAll).await,
        Err(DirectoryFailure::Unavailable)
    );
    assert_eq!(
        db.store
            .bootstrap(administrator(2, "two@example.com"))
            .await,
        Err(BootstrapError::Storage)
    );
    assert_eq!(db.store.migrate().await, Err(DirectoryFailure::Unavailable));
}
mod oidc;
mod tokens;

mod identity_checks;

mod resource_tokens;

mod resource_introspection;

mod email_verification;

mod invitations;

mod admin_catalog;

mod profiles;

mod media;

mod limiter_activation;
mod operator_accounts;
mod readiness;

mod signing_operations;

mod migrations;

mod operator_catalog;
