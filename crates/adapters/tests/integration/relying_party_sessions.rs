use super::tokens::{ISSUER, code, input, setup};
use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_adapters::tokens::{material, signer::Signer};
use darkhorse_application::{
    authentication::AuthenticationStore,
    oidc::{AuthorizationStore, Decision},
    signing::WrappedKey,
    tokens::{CodeStore, IdClaims, IdSigner, TokenStore, Tokens},
};
use darkhorse_domain::{identity::ClientId, tokens::Error};

fn sid(tokens: &Tokens) -> Uuid {
    let payload = tokens.id_token.as_ref().unwrap().split('.').nth(1).unwrap();
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    let value = Uuid::parse_str(claims["sid"].as_str().unwrap()).unwrap();
    assert!(!value.is_nil());
    value
}
async fn associations(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM relying_party_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn relying_party_sessions_converge_under_concurrent_redemption_and_survive_a_new_pool() {
    let (db, signer) = setup().await;
    let first = code(&db, [3; 32]).await;
    let second = code(&db, [4; 32]).await;
    let (left, right) = tokio::join!(
        db.store
            .redeem(input(&first), material::pair().unwrap(), ISSUER, &signer),
        db.store
            .redeem(input(&second), material::pair().unwrap(), ISSUER, &signer)
    );
    let expected = sid(&left.unwrap());
    assert_eq!(sid(&right.unwrap()), expected);
    assert_eq!(associations(&db).await, 1);
    let audits: Vec<Uuid> = sqlx::query_scalar(
        "SELECT relying_party_session_id FROM token_audit WHERE event='code_redeemed'",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(audits, vec![expected, expected]);
    let browser: Uuid = sqlx::query_scalar("SELECT public_id FROM browser_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_ne!(browser, expected);
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let mut endpoint =
        url::Url::parse(&std::env::var("DARKHORSE_TEST_DATABASE_URL").unwrap()).unwrap();
    endpoint.set_path(&name);
    let independent = PostgresStore::from_pool(PgPool::connect(endpoint.as_str()).await.unwrap());
    let third = code(&db, [5; 32]).await;
    let issued = independent
        .redeem(input(&third), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    assert_eq!(sid(&issued), expected);
    independent.close().await;
    db.store.close().await;
}

async fn exchange_for(
    db: &Database,
    signer: &Signer,
    session: u8,
    handle: u8,
    client: u128,
) -> Tokens {
    let mut request = super::oidc::request();
    request.client = ClientId::from_u128(client).unwrap();
    db.store
        .begin(request, [handle; 32], Some([session; 32]))
        .await
        .unwrap();
    db.store
        .resume([handle; 32], Some([session; 32]), Decision::Approve)
        .await
        .unwrap();
    let issued = db
        .store
        .issue(
            [handle; 32],
            Some([session; 32]),
            material::generate(material::Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let mut proof = input(&issued);
    proof.client = ClientId::from_u128(client).unwrap();
    proof.secret = if client == 32 { [9; 32] } else { [10; 32] };
    db.store
        .redeem(proof, material::pair().unwrap(), ISSUER, signer)
        .await
        .unwrap()
}
#[tokio::test]
async fn relying_party_sessions_isolate_clients_new_logins_and_principals() {
    let (db, signer) = setup().await;
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Second app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000021','00000000-0000-0000-0000-000000000011','Second client',true); INSERT INTO client_redirects VALUES('00000000-0000-0000-0000-000000000021','https://client.example/callback?fixed=1');").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000061','00000000-0000-0000-0000-000000000021',$1,0)").bind([10u8;32].as_slice()).execute(&db.pool).await.unwrap();
    let first = exchange_for(&db, &signer, 1, 3, 32).await;
    let other_app = exchange_for(&db, &signer, 1, 4, 33).await;
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let newer = exchange_for(&db, &signer, 2, 5, 32).await;
    assert_ne!(sid(&first), sid(&other_app));
    assert_ne!(sid(&first), sid(&newer));
    db.store.logout([1; 32]).await.unwrap();
    for token in [&first, &other_app] {
        assert!(matches!(
            db.store
                .userinfo(
                    material::digest(&token.access, material::Purpose::Access).unwrap(),
                    ISSUER
                )
                .await,
            Err(Error::InvalidToken)
        ));
    }
    assert!(
        db.store
            .userinfo(
                material::digest(&newer.access, material::Purpose::Access).unwrap(),
                ISSUER
            )
            .await
            .is_ok()
    );
    // A distinct principal uses the same client without sharing its session reference.
    super::insert_principal(&db, 2, true).await;
    let other = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&other, [6; 32], None).await.unwrap();
    let different_user = exchange_for(&db, &signer, 6, 7, 32).await;
    assert_ne!(sid(&different_user), sid(&newer));
    assert_eq!(associations(&db).await, 4);
    db.store.close().await;
}

struct BrokenSigner;
impl IdSigner for BrokenSigner {
    async fn sign(&self, _: WrappedKey, _: IdClaims) -> Result<String, Error> {
        Err(Error::Unavailable)
    }
}
#[tokio::test]
async fn relying_party_sessions_rollback_with_signing_or_audit_failure() {
    let (db, signer) = setup().await;
    let code = code(&db, [3; 32]).await;
    assert!(
        db.store
            .redeem(
                input(&code),
                material::pair().unwrap(),
                ISSUER,
                &BrokenSigner
            )
            .await
            .is_err()
    );
    assert_eq!(associations(&db).await, 0);
    sqlx::raw_sql("CREATE FUNCTION reject_rp_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; CREATE TRIGGER reject_rp_audit BEFORE INSERT ON token_audit FOR EACH ROW EXECUTE FUNCTION reject_rp_audit();").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
            .await
            .is_err()
    );
    assert_eq!(associations(&db).await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
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
    sqlx::query("DROP TRIGGER reject_rp_audit ON token_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    assert_eq!(associations(&db).await, 1);
    db.store.close().await;
}

#[tokio::test]
async fn relying_party_sessions_reject_rebinding_deletion_and_forged_audit_references() {
    let (db, signer) = setup().await;
    exchange_for(&db, &signer, 1, 3, 32).await;
    for statement in [
        "UPDATE relying_party_sessions SET sid=gen_random_uuid()",
        "UPDATE relying_party_sessions SET client_id=gen_random_uuid()",
        "UPDATE relying_party_sessions SET principal_id=gen_random_uuid()",
        "UPDATE relying_party_sessions SET issuer='https://other.example'",
        "UPDATE relying_party_sessions SET created_ms=created_ms+1",
        "DELETE FROM relying_party_sessions",
        "INSERT INTO token_audit(principal_id,client_id,event,occurred_ms) SELECT principal_id,client_id,'code_redeemed',created_ms FROM relying_party_sessions",
        "INSERT INTO token_audit(principal_id,client_id,event,occurred_ms,relying_party_session_id) SELECT principal_id,client_id,'code_redeemed',created_ms,gen_random_uuid() FROM relying_party_sessions",
        "INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT issuer,session_id,principal_id,client_id,created_ms FROM relying_party_sessions",
    ] {
        assert!(
            sqlx::query(statement).execute(&db.pool).await.is_err(),
            "{statement}"
        );
    }
    assert_eq!(associations(&db).await, 1);
    db.store.close().await;
}

#[tokio::test]
async fn relying_party_sessions_upgrade_preserves_existing_handles_and_historical_audit() {
    let db = Database::at_version(15).await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    // Seed the legacy schema directly: current session projection requires newer columns.
    sqlx::raw_sql("INSERT INTO browser_sessions(digest,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT decode(repeat('01',32),'hex'),p.id,c.id,p.credential_epoch,t,t,t+28800000 FROM principals p JOIN credentials c ON c.principal_id=p.id CROSS JOIN (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint t) n WHERE p.id='00000000-0000-0000-0000-000000000001'; INSERT INTO session_audit(principal_id,session_id,event,occurred_ms) SELECT principal_id,public_id,'created',created_ms FROM browser_sessions;").execute(&db.pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','App','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000010','Client',true); INSERT INTO token_audit(principal_id,client_id,event,occurred_ms) VALUES('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000020','code_redeemed',1);").execute(&db.pool).await.unwrap();
    db.store.migrate().await.unwrap();
    db.store.migrate().await.unwrap();
    assert_eq!(associations(&db).await, 0);
    assert_eq!(db.store.session([1; 32]).await.unwrap().principal, id(1));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM token_audit WHERE relying_party_session_id IS NULL AND occurred_ms=1").fetch_one(&db.pool).await.unwrap(), 1);
    db.store.close().await;
}
