use super::*;
fn request() -> Request {
    Request {
        client: crate::identity::ClientId::from_u128(1).unwrap(),
        redirect: "https://client.example/callback".into(),
        challenge: [1; 32],
        state: Some("state".into()),
        nonce: Some("nonce".into()),
        scopes: vec!["openid".into()],
        resource: None,
        prompt: Prompt::Default,
        max_age: None,
    }
}
#[test]
fn request_limits_and_explicit_resource_grants() {
    let mut r = request();
    assert_eq!(r.validate(), Ok(()));
    let mut p = ClientPolicy {
        active: true,
        revision: 0,
        application_revision: 0,
        redirects: vec![r.redirect.clone()],
        resources: vec![("urn:resource:1".into(), vec!["read".into()])],
    };
    assert_eq!(authorize_request(&r, &p), Ok(()));
    r.scopes.push("read".into());
    assert_eq!(authorize_request(&r, &p), Err(Error::InvalidScope));
    r.resource = Some("urn:resource:1".into());
    assert_eq!(authorize_request(&r, &p), Ok(()));
    r.resource = Some("urn:resource:2".into());
    assert_eq!(authorize_request(&r, &p), Err(Error::InvalidTarget));
    r = request();
    p.active = false;
    assert!(!trusted_redirect(&r, &p));
    p.active = true;
    r.redirect.push('/');
    assert!(!trusted_redirect(&r, &p));
    for mut invalid in [
        request(),
        request(),
        request(),
        request(),
        request(),
        request(),
        request(),
        request(),
    ]
    .into_iter()
    .enumerate()
    {
        match invalid.0 {
            0 => invalid.1.scopes.clear(),
            1 => invalid.1.scopes.push("openid".into()),
            2 => invalid.1.state = Some("\n".into()),
            3 => invalid.1.nonce = Some("x".repeat(257)),
            4 => invalid.1.max_age = Some(28801),
            5 => invalid.1.redirect.clear(),
            6 => invalid.1.resource = Some(String::new()),
            _ => invalid.1.scopes = vec!["x".repeat(101)],
        }
        assert_eq!(invalid.1.validate(), Err(Error::InvalidRequest));
    }
    r = request();
    r.scopes = vec!["read".into()];
    assert_eq!(authorize_request(&r, &p), Err(Error::InvalidScope));
}
#[test]
fn session_binding_is_immutable_and_reauthentication_is_explicit() {
    let session = Session {
        digest: [1; 32],
        principal: crate::identity::PrincipalId::from_u128(2).unwrap(),
        authenticated_ms: 100,
    };
    let mut r = request();
    assert_eq!(
        interaction(&r, 100, 100, None, None, Some(session), false),
        Ok(Interaction::Consent)
    );
    assert_eq!(
        interaction(&r, 100, 100, None, None, Some(session), true),
        Ok(Interaction::Ready)
    );
    assert_eq!(
        interaction(&r, 100, 100, None, None, None, false),
        Ok(Interaction::Login)
    );
    r.prompt = Prompt::None;
    assert_eq!(
        interaction(&r, 100, 100, None, None, None, false),
        Err(Error::LoginRequired)
    );
    assert_eq!(
        interaction(&r, 100, 100, None, None, Some(session), false),
        Err(Error::ConsentRequired)
    );
    r.prompt = Prompt::Consent;
    assert_eq!(
        interaction(&r, 100, 100, None, None, Some(session), true),
        Ok(Interaction::Consent)
    );
    r.prompt = Prompt::Login;
    r.max_age = Some(0);
    assert_eq!(
        interaction(&r, 100, 101, Some([1; 32]), None, Some(session), true),
        Ok(Interaction::Login)
    );
    let newer = Session {
        digest: [2; 32],
        authenticated_ms: 101,
        ..session
    };
    assert_eq!(
        interaction(&r, 100, 101, Some([1; 32]), None, Some(newer), true),
        Ok(Interaction::Ready)
    );
    assert_eq!(
        interaction(&r, 100, 101, None, Some(session), Some(newer), true),
        Err(Error::InvalidTransaction)
    );
    assert_eq!(
        interaction(&r, 100, 101, None, Some(session), None, true),
        Err(Error::InvalidTransaction)
    );
    r.prompt = Prompt::Default;
    r.max_age = Some(1);
    assert_eq!(
        interaction(&r, 100, 1101, None, None, Some(session), true),
        Ok(Interaction::Login)
    );
    assert_eq!(
        interaction(&r, 100, 99, None, None, Some(session), true),
        Err(Error::InvalidTransaction)
    );
    assert_eq!(
        interaction(&r, 100, 300100, None, None, Some(session), true),
        Err(Error::InvalidTransaction)
    );
    assert_eq!(
        interaction(&r, 100, 100, None, None, Some(newer), true),
        Err(Error::InvalidTransaction)
    );
}

#[test]
fn changed_redirect_is_rejected_and_consent_prompt_is_satisfied_only_once() {
    let r = request();
    let policy = ClientPolicy {
        active: true,
        revision: 0,
        application_revision: 0,
        redirects: vec!["https://other.example".into()],
        resources: vec![],
    };
    assert_eq!(authorize_request(&r, &policy), Err(Error::InvalidRequest));
    assert_eq!(continued_prompt(Prompt::Consent, false), Prompt::Consent);
    assert_eq!(continued_prompt(Prompt::Consent, true), Prompt::Default);
    assert_eq!(continued_prompt(Prompt::LoginConsent, true), Prompt::Login);
    assert_eq!(continued_prompt(Prompt::None, true), Prompt::None);
}

#[test]
fn approval_is_single_use_requires_login_and_capacity_never_overflows() {
    assert_eq!(
        decision(Interaction::Login, false, Decision::Approve),
        Err(Error::LoginRequired)
    );
    assert_eq!(
        decision(Interaction::Consent, true, Decision::Approve),
        Err(Error::InvalidTransaction)
    );
    assert_eq!(
        decision(Interaction::Consent, false, Decision::Approve),
        Ok(Decision::Approve)
    );
    assert_eq!(
        decision(Interaction::Login, false, Decision::Deny),
        Ok(Decision::Deny)
    );
    assert_eq!(
        decision(Interaction::Ready, true, Decision::Inspect),
        Ok(Decision::Inspect)
    );
    assert_eq!(capacity(4999, 99), Ok(()));
    assert_eq!(capacity(5000, 99), Err(Error::Unavailable));
    assert_eq!(capacity(0, 100), Err(Error::Unavailable));
}

#[test]
fn malformed_scope_sets_cannot_bypass_catalog_validation() {
    let mut r = request();
    r.scopes.clear();
    let policy = ClientPolicy {
        active: true,
        revision: 0,
        application_revision: 0,
        redirects: vec![r.redirect.clone()],
        resources: vec![],
    };
    assert_eq!(authorize_request(&r, &policy), Err(Error::InvalidRequest));
}
