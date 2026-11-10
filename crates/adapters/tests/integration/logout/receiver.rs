use super::{receiver_fixture::Receiver, *};
use crate::relying_party_sessions::exchange_for;
use darkhorse_adapters::{
    logout::{configuration, sender::HttpsSender},
    signing::{configuration::WrapKey, crypto},
};
use darkhorse_application::{
    logout::{
        LogoutSigner,
        delivery::{Sender, deliver_next},
    },
    signing::SigningStore,
};
use envbind::{Environment, EnvironmentError};
use serde_json::json;

struct Network {
    origin: String,
    ca: String,
}
impl Environment for Network {
    fn get(&self, key: &str) -> Result<Option<String>, EnvironmentError> {
        Ok(match key {
            "DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS" => {
                Some(json!({ self.origin.clone(): ["127.0.0.1", "::1"] }).to_string())
            }
            "DARKHORSE_LOGOUT_CA_PEM" => Some(self.ca.clone()),
            _ => None,
        })
    }
}
async fn publish(db: &Database, receiver: &mut Receiver) {
    let keys: Vec<_> = db
        .store
        .published(ISSUER)
        .await
        .unwrap()
        .into_iter()
        .map(|key| json!({"kid":key.kid,"n":key.n,"e":key.e,"kty":"RSA","alg":"RS256","use":"sig"}))
        .collect();
    for client in [32, 33] {
        receiver.command(json!({"command":"configure", "path":format!("/{client}"), "issuer":ISSUER,"client":Uuid::from_u128(client).to_string(),"keys":keys}));
    }
}
async fn second_application(db: &Database) {
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Second app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000021','00000000-0000-0000-0000-000000000011','Second client',true); INSERT INTO client_redirects VALUES('00000000-0000-0000-0000-000000000021','https://client.example/callback?fixed=1');").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000061','00000000-0000-0000-0000-000000000021',$1,0)").bind([10u8;32].as_slice()).execute(&db.pool).await.unwrap();
}
fn bind(receiver: &mut Receiver, client: u128, subject: u128, id: &str, token: &str) -> bool {
    receiver.command(json!({"command":"bind","path":format!("/{client}"),"subject":Uuid::from_u128(subject).to_string(),"nonce":"nonce","id":id,"token":token})).as_bool().unwrap()
}
async fn due(db: &Database) {
    sqlx::query("UPDATE logout_deliveries SET next_ms=created_ms WHERE state='queued'")
        .execute(&db.pool)
        .await
        .unwrap();
}
async fn queued(db: &Database) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM logout_deliveries WHERE state='queued'")
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn durable_logout_reaches_two_receivers_with_replay_restart_rotation_and_lost_acknowledgment()
{
    let mut receiver = Receiver::start();
    let (db, signer) = setup().await;
    second_application(&db).await;
    publish(&db, &mut receiver).await;
    for (application, client) in [(16, 32), (17, 33)] {
        db.store
            .configure_logout(
                [1; 32],
                Target {
                    application: ApplicationId::from_u128(application).unwrap(),
                    client: ClientId::from_u128(client).unwrap(),
                },
                config(0, Some(&format!("{}/{client}", receiver.origin))),
            )
            .await
            .unwrap();
    }
    let first = exchange_for(&db, &signer, 1, 3, 32).await.id_token.unwrap();
    let second = exchange_for(&db, &signer, 1, 4, 33).await.id_token.unwrap();
    assert!(bind(&mut receiver, 32, 1, "old-a", &first));
    assert!(bind(&mut receiver, 32, 1, "old-tab", &first));
    assert!(bind(&mut receiver, 33, 1, "old-b", &second));
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    let newer = exchange_for(&db, &signer, 2, 5, 32).await.id_token.unwrap();
    assert!(bind(&mut receiver, 32, 1, "newer", &newer));
    crate::insert_principal(&db, 2, true).await;
    let other = db
        .store
        .candidate("person2@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&other, [3; 32], None).await.unwrap();
    let unrelated = exchange_for(&db, &signer, 3, 6, 32).await.id_token.unwrap();
    assert!(bind(&mut receiver, 32, 2, "unrelated", &unrelated));
    let sender = HttpsSender::new(
        configuration::load(Network {
            origin: receiver.origin.clone(),
            ca: receiver.ca.clone(),
        })
        .unwrap(),
    );

    receiver.command(json!({"command":"unavailable","value":true}));
    db.store.logout([1; 32]).await.unwrap();
    for _ in 0..2 {
        assert!(deliver_next(&db.store, &signer, &sender).await.unwrap());
    }
    assert_eq!(queued(&db).await, 2);
    assert!(db.store.session([1; 32]).await.is_err());
    receiver.sessions(&["newer", "old-a", "old-b", "old-tab", "unrelated"]);
    receiver.command(json!({"command":"unavailable","value":false}));
    due(&db).await;

    let attempt = db.store.claim().await.unwrap().unwrap();
    let original_jti = attempt.claims.token;
    let wrong_client = if attempt.claims.client.as_u128() == 32 {
        33
    } else {
        32
    };
    let token = signer
        .sign_logout(attempt.key, attempt.claims)
        .await
        .unwrap();
    assert_eq!(
        sender
            .send(&format!("{}/{wrong_client}", receiver.origin), &token)
            .await,
        DeliveryResult::Rejected
    );
    receiver.sessions(&["newer", "old-a", "old-b", "old-tab", "unrelated"]);
    receiver.command(json!({"command":"lose-reply"}));
    let result = sender.send(&attempt.destination, &token).await;
    assert_eq!(result, DeliveryResult::Retry);
    db.store
        .finish(attempt.id, attempt.attempt, result)
        .await
        .unwrap();
    receiver.command(json!({"command":"restart"}));
    assert_eq!(
        sender.send(&attempt.destination, &token).await,
        DeliveryResult::Accepted
    );
    assert!(deliver_next(&db.store, &signer, &sender).await.unwrap());
    receiver.sessions(&["newer", "unrelated"]);
    assert!(!bind(&mut receiver, 32, 1, "late-callback", &first));
    assert!(!bind(&mut receiver, 33, 1, "late-callback-other", &second));

    // Prepublish then activate a different key while one accepted notification remains queued.
    let wrap = WrapKey::from_hex(&"12".repeat(32)).unwrap();
    let key = crypto::generate(ISSUER, &wrap).unwrap();
    let kid = key.public.kid.clone();
    db.store
        .stage(ISSUER, wrap.fingerprint(), 2, key)
        .await
        .unwrap();
    publish(&db, &mut receiver).await;
    sqlx::raw_sql("ALTER TABLE signing_keys DISABLE TRIGGER signing_transition; UPDATE signing_keys SET created_ms=created_ms-60001 WHERE phase='staged'; ALTER TABLE signing_keys ENABLE TRIGGER signing_transition;").execute(&db.pool).await.unwrap();
    db.store.activate(ISSUER, &kid, 3).await.unwrap();
    assert_eq!(
        sender.send(&attempt.destination, &token).await,
        DeliveryResult::Accepted
    );
    due(&db).await;
    let independent = PostgresStore::from_pool(
        PgPool::connect_with((*db.pool.connect_options()).clone())
            .await
            .unwrap(),
    );
    let retry = independent.claim().await.unwrap().unwrap();
    assert_eq!(retry.key.public.kid, kid);
    assert_ne!(retry.claims.token, original_jti);
    let retry_token = signer.sign_logout(retry.key, retry.claims).await.unwrap();
    let result = sender.send(&retry.destination, &retry_token).await;
    assert_eq!(result, DeliveryResult::Accepted);
    independent
        .finish(retry.id, retry.attempt, result)
        .await
        .unwrap();
    assert!(!deliver_next(&independent, &signer, &sender).await.unwrap());
    assert_eq!(queued(&db).await, 0);
    receiver.sessions(&["newer", "unrelated"]);
    assert!(db.store.session([2; 32]).await.is_ok());
    assert!(db.store.session([3; 32]).await.is_ok());
    independent.close().await;
    db.store.close().await;
}
