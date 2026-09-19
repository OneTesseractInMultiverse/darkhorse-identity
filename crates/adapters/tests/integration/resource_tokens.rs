use super::tokens::{ISSUER, input, setup};
use super::*;
use darkhorse_adapters::tokens::material::{self, Purpose};
use darkhorse_application::{
    oidc::{AuthorizationStore, Decision},
    tokens::{CodeStore, ManagedToken, Management, TokenManagementStore, TokenStore},
};
use darkhorse_domain::{oidc::Request, tokens::Error};

const AUDIENCE: &str = "urn:darkhorse:resource:00000000-0000-0000-0000-000000000030";
pub(super) async fn policy(db: &Database) {
    sqlx::raw_sql("INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000030');
    INSERT INTO resource_scopes VALUES('00000000-0000-0000-0000-000000000040','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','operate');
    INSERT INTO client_resources VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030');
    INSERT INTO client_scopes VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000040');
    INSERT INTO capabilities(id,permission_key,meaning) VALUES('00000000-0000-0000-0000-000000000070','read','Read API records'),('00000000-0000-0000-0000-000000000071','write','Write API records');
    INSERT INTO capability_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM capabilities;
    INSERT INTO roles(id,name) VALUES('00000000-0000-0000-0000-000000000080','Writer');
    INSERT INTO role_applications VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000080');
    INSERT INTO role_capabilities SELECT '00000000-0000-0000-0000-000000000080',id FROM capabilities;
    INSERT INTO resource_capabilities SELECT '00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030',id FROM capabilities;
    INSERT INTO scope_capabilities SELECT '00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000040',id FROM capabilities;
    INSERT INTO principal_roles VALUES('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000080');")
        .execute(&db.pool).await.unwrap();
}
fn request() -> Request {
    Request {
        resource: Some(AUDIENCE.into()),
        scopes: vec!["openid".into(), "operate".into()],
        ..super::oidc::request()
    }
}
pub(super) async fn approve(db: &Database, handle: [u8; 32]) {
    db.store
        .begin(request(), handle, Some([1; 32]))
        .await
        .unwrap();
    db.store
        .resume(handle, Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
}
#[tokio::test]
async fn resource_ceiling_only_shrinks_from_consent_to_code_to_token() {
    let (db, signer) = setup().await;
    policy(&db).await;
    approve(&db, [3; 32]).await;
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000071')").execute(&db.pool).await.unwrap();
    let token = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let (audience, ceiling): (String, Vec<Uuid>) =
        sqlx::query_as("SELECT audience,capability_ceiling FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(audience, AUDIENCE);
    assert_eq!(ceiling, vec![Uuid::from_u128(0x70)]);
    assert_eq!(token.scope, "openid operate");
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    assert!(matches!(
        db.store.userinfo(digest, ISSUER).await,
        Err(Error::InvalidToken)
    ));
    let management = || Management {
        client: request().client,
        secret: [9; 32],
        token: Some(ManagedToken::Access(digest)),
    };
    assert!(
        db.store
            .introspect(management(), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.revoke(management(), ISSUER).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT revoked FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}
#[tokio::test]
async fn registration_and_admin_membership_never_replace_role_authority() {
    let (db, _) = setup().await;
    policy(&db).await;
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .begin(request(), [3; 32], Some([1; 32]))
        .await
        .unwrap();
    assert!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await
            .is_err()
    );
    assert!(
        db.store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap()
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    db.store.close().await;
}
#[tokio::test]
async fn policy_changes_before_consent_and_empty_grants_after_consent_deny() {
    let (db, signer) = setup().await;
    policy(&db).await;
    db.store
        .begin(request(), [3; 32], Some([1; 32]))
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    assert!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await
            .is_err()
    );
    approve(&db, [4; 32]).await;
    let code = db
        .store
        .issue(
            [4; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}

#[tokio::test]
async fn new_permissions_after_approval_never_expand_the_consent_ceiling() {
    let (db, signer) = setup().await;
    policy(&db).await;
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    approve(&db, [3; 32]).await;
    sqlx::query("INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000071')").execute(&db.pool).await.unwrap();
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let mut wrong = input(&code);
    wrong.resource = Some("urn:other".into());
    assert!(matches!(
        db.store
            .redeem(wrong, material::pair().unwrap(), ISSUER, &signer)
            .await,
        Err(Error::InvalidTarget)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    let mut matched = input(&code);
    matched.resource = Some(AUDIENCE.into());
    db.store
        .redeem(matched, material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let ceiling: Vec<Uuid> = sqlx::query_scalar("SELECT capability_ceiling FROM access_tokens")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(ceiling, vec![Uuid::from_u128(0x70)]);
    for statement in [
        "UPDATE authorization_resource_grants SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000071']::uuid[]",
        "UPDATE authorization_codes SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000071']::uuid[]",
        "UPDATE access_tokens SET capability_ceiling=ARRAY['00000000-0000-0000-0000-000000000071']::uuid[]",
        "UPDATE access_tokens SET resource_id=NULL",
        "DELETE FROM authorization_policy_audit",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    assert!(matches!(
        db.store
            .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT revoked FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}
#[tokio::test]
async fn consent_audit_failure_rolls_back_the_frozen_resource_grant() {
    let (db, _) = setup().await;
    policy(&db).await;
    db.store
        .begin(request(), [3; 32], Some([1; 32]))
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_resource_consent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_resource_consent BEFORE INSERT ON consent_audit FOR EACH ROW EXECUTE FUNCTION reject_resource_consent();").execute(&db.pool).await.unwrap();
    assert!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_resource_grants")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_consents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_resource_consent ON consent_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .resume([3; 32], Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    let approved: Vec<Uuid> = sqlx::query_scalar("SELECT capability_ceiling FROM consent_audit")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(approved.len(), 2);
    db.store.close().await;
}
#[tokio::test]
async fn a_committing_permission_reduction_is_seen_by_waiting_redemption() {
    let (db, signer) = setup().await;
    policy(&db).await;
    approve(&db, [3; 32]).await;
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let mut change = db.pool.begin().await.unwrap();
    sqlx::query("DELETE FROM principal_roles")
        .execute(&mut *change)
        .await
        .unwrap();
    let operation = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer);
    tokio::pin!(operation);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut operation)
            .await
            .is_err()
    );
    change.commit().await.unwrap();
    assert!(matches!(operation.await, Err(Error::InvalidGrant)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    db.store.close().await;
}
#[tokio::test]
async fn resource_policy_constraints_and_audit_are_atomic() {
    let (db, _) = setup().await;
    policy(&db).await;
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Other','00000000-0000-0000-0000-000000000001',true)").execute(&db.pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO capabilities(id,permission_key,meaning) VALUES('00000000-0000-0000-0000-000000000072','unexposed','Not exposed by the resource'),('00000000-0000-0000-0000-000000000073','unbound','Not bound to an application'); INSERT INTO capability_applications VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000072'); INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000072');")
        .execute(&db.pool).await.unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for statement in [
        "INSERT INTO role_applications VALUES('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000080')",
        "INSERT INTO principal_roles VALUES('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000080')",
        "UPDATE roles SET id='00000000-0000-0000-0000-000000000081'",
        "DELETE FROM roles",
        "DELETE FROM capability_applications",
        "DELETE FROM capability_applications WHERE capability_id='00000000-0000-0000-0000-000000000072'",
        "INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000073')",
        "INSERT INTO scope_capabilities VALUES('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000040','00000000-0000-0000-0000-000000000070')",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        revision
    );
    sqlx::raw_sql("CREATE FUNCTION reject_policy_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_policy_audit BEFORE INSERT ON authorization_policy_audit FOR EACH ROW EXECUTE FUNCTION reject_policy_audit();").execute(&db.pool).await.unwrap();
    assert!(
        sqlx::query("DELETE FROM principal_roles")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM principal_roles")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        revision
    );
    db.store.close().await;
}
#[tokio::test]
async fn retirement_scope_changes_and_consent_withdrawal_deny_redemption() {
    for statement in [
        "UPDATE capabilities SET retired=true",
        "DELETE FROM scope_capabilities",
        "DELETE FROM client_scopes",
        "DELETE FROM oauth_consents",
    ] {
        let (db, signer) = setup().await;
        policy(&db).await;
        approve(&db, [3; 32]).await;
        let code = db
            .store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap(),
            )
            .await
            .unwrap();
        sqlx::query(statement).execute(&db.pool).await.unwrap();
        assert!(matches!(
            db.store
                .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
                .await,
            Err(Error::InvalidGrant)
        ));
        assert!(
            !sqlx::query_scalar::<_, bool>("SELECT consumed FROM authorization_codes")
                .fetch_one(&db.pool)
                .await
                .unwrap()
        );
        db.store.close().await;
    }
}
#[tokio::test]
async fn policy_projection_rejects_oversize_inputs_instead_of_truncating_authority() {
    for statement in [
        "INSERT INTO capabilities(id,permission_key,meaning) SELECT lpad(to_hex(i),32,'0')::uuid,'extra-'||i,'test' FROM generate_series(1000,1254) AS i; INSERT INTO capability_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM capabilities WHERE permission_key LIKE 'extra-%'; INSERT INTO resource_capabilities SELECT '00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030',id FROM capabilities WHERE permission_key LIKE 'extra-%';",
        "INSERT INTO roles(id,name) SELECT lpad(to_hex(i),32,'0')::uuid,'extra' FROM generate_series(1000,1063) AS i; INSERT INTO role_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='extra'; INSERT INTO principal_roles SELECT '00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='extra';",
    ] {
        let (db, _) = setup().await;
        policy(&db).await;
        sqlx::raw_sql(statement).execute(&db.pool).await.unwrap();
        db.store
            .begin(request(), [3; 32], Some([1; 32]))
            .await
            .unwrap();
        assert!(matches!(
            db.store
                .resume([3; 32], Some([1; 32]), Decision::Approve)
                .await,
            Err(darkhorse_domain::oidc::Error::Unavailable)
        ));
        db.store.close().await;
    }
}

#[tokio::test]
async fn http_resource_flow_binds_the_target_and_separates_userinfo_from_api_authority() {
    use axum::{
        body::{Body, to_bytes},
        http::Request as HttpRequest,
    };
    use base64::{
        Engine as _,
        engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    };
    use darkhorse_application::authentication::AuthenticationStore;
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    let (db, signer) = setup().await;
    policy(&db).await;
    let secret = "ab".repeat(32);
    sqlx::query("UPDATE oauth_client_secrets SET retired=true")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000061','00000000-0000-0000-0000-000000000020',$1,0)")
        .bind(darkhorse_adapters::registration::secret_digest(&secret).unwrap().as_slice()).execute(&db.pool).await.unwrap();
    let raw = "01".repeat(32);
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store
        .establish(&candidate, Sha256::digest(raw.as_bytes()).into(), None)
        .await
        .unwrap();
    let origin = url::Url::parse(ISSUER).unwrap();
    let browser = darkhorse_adapters::provider_http::router(db.store.clone(), origin.clone());
    let backend = darkhorse_adapters::token_http::router(db.store.clone(), signer, origin);
    let verifier = "a".repeat(43);
    let mut authorize = url::Url::parse(&format!("{ISSUER}/authorize")).unwrap();
    authorize.query_pairs_mut().extend_pairs([
        ("client_id", "00000000-0000-0000-0000-000000000020"),
        ("redirect_uri", request().redirect.as_str()),
        ("response_type", "code"),
        ("scope", "openid operate"),
        ("resource", AUDIENCE),
        ("state", "resource-test"),
        ("nonce", "nonce-test"),
        ("code_challenge_method", "S256"),
        (
            "code_challenge",
            URL_SAFE_NO_PAD
                .encode(material::challenge(&verifier).unwrap())
                .as_str(),
        ),
    ]);
    let response = browser
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri(format!("/authorize?{}", authorize.query().unwrap()))
                .header("host", "issuer.example")
                .header("cookie", format!("__Host-darkhorse={raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let cookies = format!("__Host-darkhorse={raw}; {cookie}");
    let response = browser
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/api/authorization")
                .header("host", "issuer.example")
                .header("cookie", &cookies)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let view: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(view["status"], "consent");
    assert_eq!(view["resource"], AUDIENCE);
    assert_eq!(view["scopes"], serde_json::json!(["openid", "operate"]));
    let response = browser
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/api/authorization/decision")
                .header("host", "issuer.example")
                .header("origin", ISSUER)
                .header("cookie", &cookies)
                .header("x-darkhorse-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"request_id":view["request_id"],"decision":"approve"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let callback = url::Url::parse(response["redirect"].as_str().unwrap()).unwrap();
    let params = callback
        .query_pairs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(params["iss"], ISSUER);
    assert_eq!(params["state"], "resource-test");
    let code = &params["code"];
    let basic = format!(
        "Basic {}",
        STANDARD.encode(format!("00000000-0000-0000-0000-000000000020:{secret}"))
    );
    let form = |target: &str| {
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("grant_type", "authorization_code"),
                ("code", code.as_ref()),
                ("redirect_uri", request().redirect.as_str()),
                ("code_verifier", verifier.as_str()),
                ("resource", target),
            ])
            .finish()
    };
    let post = |path: &str, body: String| {
        HttpRequest::builder()
            .method("POST")
            .uri(path)
            .header("host", "issuer.example")
            .header("authorization", &basic)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let response = backend
        .clone()
        .oneshot(post("/token", form("urn:wrong")))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let failure: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(failure, serde_json::json!({"error":"invalid_target"}));
    let response = backend
        .clone()
        .oneshot(post("/token", form(AUDIENCE)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let token: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(token["scope"], "openid operate");
    assert!(token["access_token"].as_str().unwrap().starts_with("da_"));
    assert!(token.get("refresh_token").is_none());
    let access = token["access_token"].as_str().unwrap();
    let digest = material::digest(access, Purpose::Access).unwrap();
    let stored: (String, Vec<uuid::Uuid>) =
        sqlx::query_as("SELECT audience,capability_ceiling FROM access_tokens WHERE digest=$1")
            .bind(digest.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(stored.0, AUDIENCE);
    assert_eq!(stored.1.len(), 2);
    for bearer in [access, token["id_token"].as_str().unwrap()] {
        let response = backend
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/userinfo")
                    .header("host", "issuer.example")
                    .header("authorization", format!("Bearer {bearer}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }
    let response = backend
        .clone()
        .oneshot(post("/introspect", format!("token={access}")))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let active: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(active, serde_json::json!({"active":false}));
    let response = backend
        .oneshot(post("/revoke", format!("token={access}")))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT revoked FROM access_tokens WHERE digest=$1")
            .bind(digest.as_slice())
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}

#[tokio::test]
async fn a_scope_reduction_at_redemption_narrows_the_token_without_rewriting_the_code() {
    let (db, signer) = setup().await;
    policy(&db).await;
    approve(&db, [3; 32]).await;
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM scope_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let issued = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let code_ceiling: Vec<Uuid> =
        sqlx::query_scalar("SELECT capability_ceiling FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token_ceiling: Vec<Uuid> =
        sqlx::query_scalar("SELECT capability_ceiling FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        code_ceiling,
        vec![Uuid::from_u128(0x70), Uuid::from_u128(0x71)]
    );
    assert_eq!(token_ceiling, vec![Uuid::from_u128(0x70)]);
    let token = material::digest(&issued.access, Purpose::Access).unwrap();
    db.store
        .revoke(
            Management {
                client: request().client,
                secret: [9; 32],
                token: Some(ManagedToken::Access(token)),
            },
            "https://other.example",
        )
        .await
        .unwrap();
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT revoked FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    db.store.close().await;
}

#[tokio::test]
async fn failed_resource_storage_and_post_consent_permission_loss_never_partially_issue() {
    let (db, signer) = setup().await;
    policy(&db).await;
    db.store
        .begin(request(), [3; 32], Some([1; 32]))
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_resource_grant() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_resource_grant BEFORE INSERT ON authorization_resource_grants FOR EACH ROW EXECUTE FUNCTION reject_resource_grant();")
        .execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await,
        Err(darkhorse_domain::oidc::Error::Unavailable)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT approved FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_consents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_resource_grant ON authorization_resource_grants")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .resume([3; 32], Some([1; 32]), Decision::Approve)
        .await
        .unwrap();
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .issue(
                [3; 32],
                Some([1; 32]),
                material::generate(Purpose::Code).unwrap()
            )
            .await,
        Err(Error::InvalidGrant)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT terminal FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("INSERT INTO principal_roles VALUES('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000080')").execute(&db.pool).await.unwrap();
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    sqlx::query("ALTER TABLE resource_capabilities RENAME COLUMN capability_id TO unavailable")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
            .await,
        Err(Error::Unavailable)
    ));
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT consumed FROM authorization_codes")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM access_tokens")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("ALTER TABLE resource_capabilities RENAME COLUMN unavailable TO capability_id")
        .execute(&db.pool)
        .await
        .unwrap();
    db.store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    db.store.close().await;
}

#[tokio::test]
async fn database_rejects_access_grants_that_exceed_or_change_the_code_profile() {
    let (db, signer) = setup().await;
    policy(&db).await;
    approve(&db, [3; 32]).await;
    let resource_code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let identity_code = super::tokens::code(&db, [4; 32]).await;
    for (code, resource, scope, claims, capabilities, consumed) in [
        (&resource_code, None, "openid", vec!["sub"], vec![], true),
        (
            &resource_code,
            Some(Uuid::from_u128(0x30)),
            "openid operate",
            vec![],
            vec![Uuid::from_u128(0x99)],
            true,
        ),
        (
            &resource_code,
            Some(Uuid::from_u128(0x30)),
            "openid extra",
            vec![],
            vec![Uuid::from_u128(0x70)],
            true,
        ),
        (
            &resource_code,
            Some(Uuid::from_u128(0x30)),
            "openid operate",
            vec![],
            vec![Uuid::from_u128(0x70)],
            false,
        ),
        (
            &identity_code,
            None,
            "openid email",
            vec!["sub", "email", "email_verified"],
            vec![],
            true,
        ),
    ] {
        let digest = material::digest(&code.value, Purpose::Code).unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        if consumed {
            sqlx::query("UPDATE authorization_codes SET consumed=true WHERE digest=$1")
                .bind(digest.as_slice())
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        let result = sqlx::query("INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms,resource_id) VALUES($1,$2,$3,$4,$5,$6,0,300000,$7)")
            .bind([0x55u8;32].as_slice()).bind(digest.as_slice())
            .bind(if resource.is_some() { AUDIENCE.to_string() } else { format!("{ISSUER}/userinfo") })
            .bind(scope).bind(claims).bind(capabilities).bind(resource).execute(&mut *tx).await;
        let error = result.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        tx.rollback().await.unwrap();
    }
    db.store
        .redeem(
            input(&resource_code),
            material::pair().unwrap(),
            ISSUER,
            &signer,
        )
        .await
        .unwrap();
    db.store.close().await;
}
