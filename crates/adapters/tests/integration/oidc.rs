use super::*;
use darkhorse_application::{
    authentication::AuthenticationStore,
    oidc::{AuthorizationStore, Decision, Outcome},
};
use darkhorse_domain::{
    identity::ClientId,
    oidc::{Error, Interaction, Prompt, Request},
};
pub(super) async fn fixture() -> Database {
    let db = Database::new().await;
    db.store
        .bootstrap(administrator(1, "one@example.com"))
        .await
        .unwrap();
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [1; 32], None).await.unwrap();
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','App','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000010','Client',true); INSERT INTO client_redirects VALUES('00000000-0000-0000-0000-000000000020','https://client.example/callback?fixed=1');").execute(&db.pool).await.unwrap();
    db
}
pub(super) fn request() -> Request {
    Request {
        client: ClientId::from_u128(32).unwrap(),
        redirect: "https://client.example/callback?fixed=1".into(),
        challenge: [7; 32],
        state: Some("s&x=y".into()),
        nonce: Some("nonce".into()),
        scopes: vec!["openid".into()],
        resource: None,
        prompt: Prompt::Default,
        max_age: None,
        ui_locale: None,
    }
}
fn pending(outcome: Outcome, expected: Interaction) {
    match outcome {
        Outcome::Pending(view) => assert_eq!(view.interaction, expected),
        _ => panic!("expected pending"),
    }
}
#[tokio::test]
async fn requests_are_browser_bound_immutable_and_recheck_committed_revocation() {
    let db = fixture().await;
    pending(
        db.store.begin(request(), [3; 32], None).await.unwrap(),
        Interaction::Login,
    );
    pending(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Inspect)
            .await
            .unwrap(),
        Interaction::Consent,
    );
    assert!(matches!(
        db.store.resume([3; 32], None, Decision::Approve).await,
        Err(Error::InvalidTransaction)
    ));
    for sql in [
        "UPDATE authorization_requests SET nonce='other'",
        "UPDATE authorization_requests SET challenge=decode(repeat('00',32),'hex')",
        "UPDATE authorization_requests SET redirect_uri='https://evil.example'",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    pending(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await
            .unwrap(),
        Interaction::Ready,
    );
    pending(
        db.store
            .begin(request(), [4; 32], Some([1; 32]))
            .await
            .unwrap(),
        Interaction::Ready,
    );
    db.store.logout([1; 32]).await.unwrap();
    assert!(matches!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::InvalidTransaction)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn untrusted_redirects_never_escape_and_silent_requests_fail_without_interaction() {
    let db = fixture().await;
    let mut r = request();
    r.redirect = "https://evil.example".into();
    assert!(matches!(
        db.store.begin(r, [3; 32], None).await,
        Err(Error::InvalidRequest)
    ));
    let mut r = request();
    r.scopes.push("admin".into());
    assert!(matches!(
        db.store.begin(r, [3; 32], None).await,
        Ok(Outcome::Return {
            error: Error::InvalidScope,
            ..
        })
    ));
    let mut r = request();
    r.prompt = Prompt::None;
    assert!(matches!(
        db.store.begin(r.clone(), [3; 32], None).await,
        Ok(Outcome::Return {
            error: Error::LoginRequired,
            ..
        })
    ));
    assert!(matches!(
        db.store.begin(r, [4; 32], Some([1; 32])).await,
        Ok(Outcome::Return {
            error: Error::ConsentRequired,
            ..
        })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    db.store.close().await;
}

#[tokio::test]
async fn expiry_session_substitution_stale_registration_and_denial_are_terminal() {
    let db = fixture().await;
    pending(
        db.store
            .begin(request(), [3; 32], Some([1; 32]))
            .await
            .unwrap(),
        Interaction::Consent,
    );
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    assert!(matches!(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Approve)
            .await,
        Err(Error::InvalidTransaction)
    ));
    assert!(matches!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Deny)
            .await,
        Ok(Outcome::Return {
            error: Error::AccessDenied,
            ..
        })
    ));
    assert!(matches!(
        db.store
            .resume([3; 32], Some([1; 32]), Decision::Approve)
            .await,
        Err(Error::InvalidTransaction)
    ));
    db.store
        .begin(request(), [4; 32], Some([1; 32]))
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE authorization_requests DISABLE TRIGGER authorization_transition; UPDATE authorization_requests SET created_ms=created_ms-300001,expires_ms=expires_ms-300001 WHERE NOT terminal; ALTER TABLE authorization_requests ENABLE TRIGGER authorization_transition;").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .resume([4; 32], Some([1; 32]), Decision::Inspect)
            .await,
        Err(Error::InvalidTransaction)
    ));
    db.store
        .begin(request(), [5; 32], Some([1; 32]))
        .await
        .unwrap();
    sqlx::query("UPDATE oauth_clients SET revision=revision+1,active=false")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .resume([5; 32], Some([1; 32]), Decision::Approve)
            .await,
        Err(Error::InvalidTransaction)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn forced_reauthentication_consent_expansion_and_failed_audit_preserve_authority() {
    let db = fixture().await;
    let mut r = request();
    r.prompt = Prompt::LoginConsent;
    r.max_age = Some(0);
    pending(
        db.store.begin(r, [3; 32], Some([1; 32])).await.unwrap(),
        Interaction::Login,
    );
    let candidate = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&candidate, [2; 32], None).await.unwrap();
    pending(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Inspect)
            .await
            .unwrap(),
        Interaction::Consent,
    );
    sqlx::raw_sql("CREATE FUNCTION reject_consent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'audit unavailable'; END $$; CREATE TRIGGER reject_consent BEFORE INSERT ON consent_audit FOR EACH ROW EXECUTE FUNCTION reject_consent();").execute(&db.pool).await.unwrap();
    assert!(matches!(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Approve)
            .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM oauth_consents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_consent ON consent_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    pending(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Approve)
            .await
            .unwrap(),
        Interaction::Ready,
    );
    pending(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Inspect)
            .await
            .unwrap(),
        Interaction::Ready,
    );
    assert!(matches!(
        db.store
            .resume([3; 32], Some([2; 32]), Decision::Approve)
            .await,
        Err(Error::InvalidTransaction)
    ));
    sqlx::raw_sql("INSERT INTO protected_resources VALUES('00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000010','API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000030'); INSERT INTO resource_scopes VALUES('00000000-0000-0000-0000-000000000040','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','read'); INSERT INTO client_resources VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030'); INSERT INTO client_scopes VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000040');").execute(&db.pool).await.unwrap();
    let mut r = request();
    r.resource = Some("urn:darkhorse:resource:00000000-0000-0000-0000-000000000030".into());
    r.scopes.push("read".into());
    pending(
        db.store.begin(r, [4; 32], Some([2; 32])).await.unwrap(),
        Interaction::Consent,
    );
    sqlx::query("DELETE FROM client_scopes")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(matches!(
        db.store
            .resume([4; 32], Some([2; 32]), Decision::Approve)
            .await,
        Err(Error::InvalidTransaction)
    ));
    db.store.close().await;
}
#[tokio::test]
async fn concurrent_capacity_checks_and_duplicate_handles_never_overfill_requests() {
    let db = fixture().await;
    for n in 1..100 {
        db.store.begin(request(), [n; 32], None).await.unwrap();
    }
    let (left, right) = tokio::join!(
        db.store.begin(request(), [100; 32], None),
        db.store.begin(request(), [101; 32], None)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM authorization_requests")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        100
    );
    assert!(db.store.begin(request(), [1; 32], None).await.is_err());
    db.store.close().await;
    assert!(matches!(
        db.store.begin(request(), [102; 32], None).await,
        Err(Error::Unavailable)
    ));
}

#[tokio::test]
async fn transport_enforces_origin_cookie_confirmation_and_no_false_discovery() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request as HttpRequest, StatusCode, header},
    };
    use darkhorse_adapters::provider_http;
    use darkhorse_application::signing::SigningStore;
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;
    let db = fixture().await;
    db.store
        .bind_provider("https://issuer.example", [1; 32])
        .await
        .unwrap();
    let raw = "01".repeat(32);
    let session = Sha256::digest(raw.as_bytes()).into();
    let c = db
        .store
        .candidate("one@example.com")
        .await
        .unwrap()
        .unwrap();
    db.store.establish(&c, session, None).await.unwrap();
    let router = provider_http::router(
        db.store.clone(),
        url::Url::parse("https://issuer.example").unwrap(),
    );
    let path = format!(
        "/authorize?client_id=00000000-0000-0000-0000-000000000020&redirect_uri=https%3A%2F%2Fclient.example%2Fcallback%3Ffixed%3D1&response_type=code&scope=openid&code_challenge={}&code_challenge_method=S256",
        "A".repeat(43)
    );
    let request = HttpRequest::builder()
        .uri(&path)
        .header("host", "issuer.example")
        .header("cookie", format!("__Host-darkhorse={raw}"))
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    let reference = location.strip_prefix("/authorization?request=").unwrap();
    assert_eq!(reference.len(), 64);
    let inspect_path = format!("/api/authorization?request={reference}");
    let decision_path = format!("/api/authorization/decision?request={reference}");
    let transaction = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let cookies = format!("__Host-darkhorse={raw}; {transaction}");
    let response = router
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri(&inspect_path)
                .header("host", "issuer.example")
                .header("cookie", &cookies)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let view: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(view["status"], "consent");
    for (id, csrf, expected) in [
        ("wrong", true, 400),
        (view["request_id"].as_str().unwrap(), false, 403),
        (view["request_id"].as_str().unwrap(), true, 200),
    ] {
        let mut request = HttpRequest::builder()
            .uri(&decision_path)
            .method("POST")
            .header("host", "issuer.example")
            .header("origin", "https://issuer.example")
            .header("cookie", &cookies)
            .header("content-type", "application/json");
        if csrf {
            request = request.header("x-darkhorse-csrf", "1");
        }
        let response = router
            .clone()
            .oneshot(
                request
                    .body(Body::from(
                        serde_json::json!({"request_id":id,"decision":"approve"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
    }
    for (path, host, method, expected) in [
        (
            "/.well-known/openid-configuration",
            "issuer.example",
            "GET",
            404,
        ),
        ("/jwks", "issuer.example", "GET", 200),
        ("/jwks", "spoofed.example", "GET", 403),
        (&path, "issuer.example", "HEAD", 405),
    ] {
        let response = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(path)
                    .method(method)
                    .header("host", host)
                    .header("forwarded", "host=evil.example;proto=http")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        assert!(response.headers().get(header::SET_COOKIE).is_none());
    }
    db.store.close().await;
}

#[tokio::test]
async fn language_is_immutable_per_transaction_and_never_changes_authority() {
    use darkhorse_domain::localization::Locale;
    let db = fixture().await;
    let mut spanish = request();
    spanish.ui_locale = Some(Locale::Spanish);
    let mut english = request();
    english.ui_locale = Some(Locale::English);
    for (handle, request, expected) in [
        ([50; 32], spanish, Some(Locale::Spanish)),
        ([51; 32], english, Some(Locale::English)),
        ([52; 32], request(), None),
    ] {
        let Outcome::Pending(view) = db.store.begin(request, handle, None).await.unwrap() else {
            panic!("pending request")
        };
        assert_eq!(view.ui_locale, expected);
        assert_eq!(view.interaction, Interaction::Login);
        let Outcome::Pending(view) = db
            .store
            .resume(handle, Some([1; 32]), Decision::Inspect)
            .await
            .unwrap()
        else {
            panic!("pending consent")
        };
        assert_eq!(view.ui_locale, expected);
        assert_eq!(view.scopes, ["openid"]);
        assert_eq!(view.interaction, Interaction::Consent);
    }
    assert!(
        sqlx::query("UPDATE authorization_requests SET ui_locale='en' WHERE digest=$1")
            .bind([50; 32].as_slice())
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE authorization_requests SET ui_locale='unsupported'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.logout([1; 32]).await.unwrap();
    for handle in [[50; 32], [51; 32]] {
        assert!(matches!(
            db.store
                .resume(handle, Some([1; 32]), Decision::Inspect)
                .await,
            Err(Error::InvalidTransaction)
        ));
    }
    db.store.close().await;
}

#[tokio::test]
async fn language_migration_preserves_existing_pending_request_bytes() {
    let db = Database::at_version(33).await;
    db.store
        .bootstrap(administrator(1, "migration@example.com"))
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000010','Existing app','00000000-0000-0000-0000-000000000001',true); INSERT INTO oauth_clients(id,application_id,name,active) VALUES('00000000-0000-0000-0000-000000000020','00000000-0000-0000-0000-000000000010','Existing client',true); INSERT INTO authorization_requests(digest,client_id,client_revision,application_revision,redirect_uri,challenge,state,nonce,scopes,prompt,created_ms,expires_ms) VALUES(decode(repeat('40',32),'hex'),'00000000-0000-0000-0000-000000000020',0,0,'https://client.example/callback',decode(repeat('41',32),'hex'),'opaque-state','opaque-nonce',ARRAY['openid'],'consent',100,300100);").execute(&db.pool).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT to_jsonb(r)::text FROM authorization_requests r")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.store.migrate().await.unwrap();
    let after: String =
        sqlx::query_scalar("SELECT (to_jsonb(r)-'ui_locale')::text FROM authorization_requests r")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let absent: bool = sqlx::query_scalar("SELECT ui_locale IS NULL FROM authorization_requests")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(absent);
    let invalid = sqlx::query("INSERT INTO authorization_requests(digest,client_id,client_revision,application_revision,redirect_uri,challenge,scopes,prompt,created_ms,expires_ms,ui_locale) SELECT decode(repeat('42',32),'hex'),client_id,client_revision,application_revision,redirect_uri,challenge,scopes,prompt,created_ms,expires_ms,'unsupported' FROM authorization_requests").execute(&db.pool).await;
    assert!(invalid.is_err());
    db.store.close().await;
}
