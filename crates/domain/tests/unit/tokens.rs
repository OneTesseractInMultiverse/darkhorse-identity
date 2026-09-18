use super::*;
#[test]
fn code_bindings_and_exact_lifetime_are_required() {
    let client = crate::identity::ClientId::from_u128(1).unwrap();
    let facts = CodeFacts {
        client,
        redirect: "https://app.example/cb",
        challenge: [3; 32],
        created_ms: 1000,
        expires_ms: 61000,
    };
    let mut proof = Proof {
        client,
        redirect: "https://app.example/cb",
        challenge: [3; 32],
    };
    assert_eq!(exchange(&facts, &proof, 1000), Ok(()));
    assert_eq!(exchange(&facts, &proof, 61000), Err(Error::InvalidGrant));
    assert_eq!(exchange(&facts, &proof, 999), Err(Error::InvalidGrant));
    proof.challenge = [4; 32];
    assert!(exchange(&facts, &proof, 2000).is_err());
    proof.challenge = [3; 32];
    proof.redirect = "https://app.example/cb/";
    assert!(exchange(&facts, &proof, 2000).is_err());
    proof.redirect = facts.redirect;
    proof.client = crate::identity::ClientId::from_u128(2).unwrap();
    assert!(exchange(&facts, &proof, 2000).is_err());
    assert_eq!(deadline(1000, CODE_MS), Ok(61000));
    assert!(deadline(i64::MAX as u64, CODE_MS).is_err());
    assert!(deadline(u64::MAX, CODE_MS).is_err());
}
#[test]
fn protocol_claim_ceiling_never_grants_resource_permissions() {
    assert_eq!(profile(&["openid".into()], None), Ok(()));
    assert_eq!(
        profile(&["openid".into(), "read".into()], None),
        Err(Error::InvalidScope)
    );
    assert_eq!(
        profile(&["openid".into()], Some("urn:api")),
        Err(Error::InvalidScope)
    );
    assert_eq!(profile(&[], None), Err(Error::InvalidScope));
    assert!(live(1000, 301000, 1000));
    assert!(!live(1000, 301000, 301000));
    assert!(!live(1000, 301000, 999));
    assert!(!live(1000, 301001, 2000));
    assert!(!live(1000, 1000, 1000));
}
#[test]
fn replay_binding_survives_expiration_without_allowing_a_fresh_exchange() {
    let client = ClientId::from_u128(1).unwrap();
    let facts = CodeFacts {
        client,
        redirect: "https://app.example/cb",
        challenge: [3; 32],
        created_ms: 1000,
        expires_ms: 61000,
    };
    let proof = Proof {
        client,
        redirect: facts.redirect,
        challenge: facts.challenge,
    };
    assert_eq!(binding(&facts, &proof), Ok(()));
    assert_eq!(exchange(&facts, &proof, 61001), Err(Error::InvalidGrant));
    let excessive = CodeFacts {
        expires_ms: 61001,
        ..facts
    };
    assert_eq!(exchange(&excessive, &proof, 2000), Err(Error::InvalidGrant));
}
#[test]
fn access_requires_current_registration_and_the_original_live_session() {
    use crate::{
        identity::PrincipalId,
        oidc::{ClientPolicy, Session},
    };
    let principal = PrincipalId::from_u128(1).unwrap();
    let grant = Grant {
        client_revision: 1,
        application_revision: 2,
        principal,
        authenticated_ms: 100,
    };
    for scenario in 0..8 {
        let mut policy = ClientPolicy {
            active: true,
            revision: 1,
            application_revision: 2,
            redirects: vec!["https://app.example/cb".into()],
            resources: vec![],
        };
        let mut session = Session {
            digest: [3; 32],
            principal,
            authenticated_ms: 100,
        };
        match scenario {
            1 => policy.active = false,
            2 => policy.revision += 1,
            3 => policy.application_revision += 1,
            4 => policy.redirects.clear(),
            5 => session.principal = PrincipalId::from_u128(2).unwrap(),
            6 => session.authenticated_ms += 1,
            _ => (),
        }
        let result = current(
            &grant,
            &policy,
            (scenario != 7).then_some(session),
            "https://app.example/cb",
        );
        assert_eq!(
            result,
            if scenario == 0 {
                Ok(())
            } else {
                Err(Error::InvalidGrant)
            }
        );
    }
}
