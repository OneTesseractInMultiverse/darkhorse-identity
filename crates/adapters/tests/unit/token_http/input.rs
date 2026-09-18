use super::*;
#[test]
fn basic_uses_form_decoding_and_rejects_multiple_authentication_methods() {
    let id = "00000000-0000-0000-0000-000000000001";
    let secret = "ab".repeat(32);
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!(
            "Basic {}",
            STANDARD.encode(format!("{}:{}", id.replace('-', "%2D"), secret))
        )
        .parse()
        .unwrap(),
    );
    let parsed = basic(&headers).unwrap();
    assert_eq!(parsed.0.as_u128(), 1);
    headers.append("authorization", "Basic bad".parse().unwrap());
    assert_eq!(basic(&headers), Err(Error::InvalidClient));
    for raw in [
        "abc",
        "grant_type=password",
        "grant_type=authorization_code&code=x&code=x",
    ] {
        assert!(form(raw).is_err());
    }
    let mut header = HeaderMap::new();
    header.insert("authorization", "Bearer eyJ.test.token".parse().unwrap());
    assert!(bearer(&header).is_err());
}
#[test]
fn token_input_keeps_authentication_and_credential_purposes_separate() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded; charset=UTF-8"
            .parse()
            .unwrap(),
    );
    headers.insert(
        "authorization",
        format!(
            "Basic {}",
            STANDARD.encode(format!(
                "00000000-0000-0000-0000-000000000001:{}",
                "ab".repeat(32)
            ))
        )
        .parse()
        .unwrap(),
    );
    let body = format!(
        "grant_type=authorization_code&code=dc_{}&redirect_uri=https%3A%2F%2Fapp.example%2Fcb&code_verifier={}",
        "ab".repeat(32),
        "a".repeat(43)
    );
    let parsed = request(&headers, body.as_bytes()).unwrap();
    assert_eq!(parsed.redirect, "https://app.example/cb");
    assert_eq!(parsed.resource, None);
    assert_eq!(
        request(
            &headers,
            format!("{body}&resource=urn%3Adarkhorse%3Aresource%3Atarget").as_bytes()
        )
        .unwrap()
        .resource
        .as_deref(),
        Some("urn:darkhorse:resource:target")
    );
    assert_eq!(
        parsed.challenge,
        material::challenge(&"a".repeat(43)).unwrap()
    );
    for added in [
        "&code_verifier=duplicate",
        "&client_secret=secret",
        "&client_id=other",
        "&client_assertion=x",
        "&client_assertion_type=x",
        "&bad=%GG",
        "&resource=urn%3Aone&resource=urn%3Atwo",
    ] {
        assert!(matches!(
            request(&headers, format!("{body}{added}").as_bytes()),
            Err(Error::InvalidRequest)
        ));
    }
    assert!(request(&headers, body.replace("dc_", "da_").as_bytes()).is_err());
    assert!(request(&headers, b"grant_type=authorization_code").is_err());
    headers.insert("content-type", "application/json".parse().unwrap());
    assert!(matches!(
        request(&headers, body.as_bytes()),
        Err(Error::InvalidRequest)
    ));
    for value in [
        "Basic bad!",
        "Basic YTpi",
        "Bearer token",
        "Basic",
        "Basic /w==",
    ] {
        headers.insert("authorization", value.parse().unwrap());
        assert_eq!(basic(&headers), Err(Error::InvalidClient));
    }
    headers.insert(
        "authorization",
        format!("Bearer da_{}", "ab".repeat(32)).parse().unwrap(),
    );
    assert_eq!(
        bearer(&headers),
        material::digest(&format!("da_{}", "ab".repeat(32)), Purpose::Access)
    );
    assert!(form(&"x".repeat(4097)).is_err());
    assert!(
        form(&format!(
            "grant_type=authorization_code{}",
            "&x=y".repeat(17)
        ))
        .is_err()
    );
}

#[test]
fn management_requires_authentication_but_wrong_token_types_remain_opaque() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    headers.insert(
        "authorization",
        format!(
            "Basic {}",
            STANDARD.encode(format!(
                "00000000-0000-0000-0000-000000000001:{}",
                "ab".repeat(32)
            ))
        )
        .parse()
        .unwrap(),
    );
    for token in ["eyJ.test.jwt", "unknown", "dc_test", "browser-handle"] {
        let parsed = management(
            &headers,
            format!("token={token}&token_type_hint=refresh_token").as_bytes(),
        )
        .unwrap();
        assert_eq!(parsed.client.as_u128(), 1);
        assert_eq!(parsed.token, None);
    }
    let token = format!("da_{}", "ab".repeat(32));
    assert_eq!(
        management(&headers, format!("token={token}").as_bytes())
            .unwrap()
            .token,
        Some(material::digest(&token, Purpose::Access).unwrap())
    );
    for body in [
        "",
        "token=",
        "token=x&token=y",
        "token=x&client_secret=secret",
    ] {
        assert!(management(&headers, body.as_bytes()).is_err());
    }
    headers.remove("authorization");
    assert!(matches!(
        management(&headers, b"token=unknown"),
        Err(Error::InvalidClient)
    ));
}

#[test]
fn introspection_credentials_have_an_explicit_purpose_and_cannot_authenticate_token_exchange() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    headers.insert(
        "authorization",
        format!(
            "Basic {}",
            STANDARD.encode(format!(
                "rs_00000000-0000-0000-0000-000000000030:{}",
                "ab".repeat(32)
            ))
        )
        .parse()
        .unwrap(),
    );
    let input = introspection(&headers, b"token=unknown").unwrap();
    match input {
        Inquiry::Resource(probe) => {
            assert_eq!(probe.resource.as_u128(), 0x30);
            assert_eq!(
                probe.secret,
                crate::resource_servers::secret_digest(&"ab".repeat(32)).unwrap()
            );
            assert_eq!(probe.token, None);
        }
        _ => panic!("expected resource credential"),
    }
    assert!(basic(&headers).is_err());
    assert!(management(&headers, b"token=unknown").is_err());
    assert!(introspection(&headers, b"token=unknown&token=duplicate").is_err());
    assert_ne!(
        crate::resource_servers::secret_digest(&"ab".repeat(32)).unwrap(),
        crate::registration::secret_digest(&"ab".repeat(32)).unwrap()
    );
}

#[test]
fn resource_authentication_rejects_noncanonical_identifiers_and_secret_substitution() {
    for (id, secret) in [
        ("rs_00000000-0000-0000-0000-000000000000", "ab".repeat(32)),
        ("rs_00000000000000000000000000000030", "ab".repeat(32)),
        ("rs_00000000-0000-0000-0000-0000000000AB", "ab".repeat(32)),
        ("rs_invalid", "ab".repeat(32)),
        ("rs_00000000-0000-0000-0000-000000000030", "AB".repeat(32)),
        ("rs_00000000-0000-0000-0000-000000000030", "da_token".into()),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        headers.insert(
            "authorization",
            format!("Basic {}", STANDARD.encode(format!("{id}:{secret}")))
                .parse()
                .unwrap(),
        );
        assert!(matches!(
            introspection(&headers, b"token=unknown"),
            Err(Error::InvalidClient)
        ));
    }
}
