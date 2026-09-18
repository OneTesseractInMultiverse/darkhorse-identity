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
