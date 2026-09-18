use super::*;
fn query() -> String {
    format!(
        "client_id=00000000-0000-0000-0000-000000000020&redirect_uri=https%3A%2F%2Fclient.example%2Fcb&response_type=code&scope=openid&code_challenge={}&code_challenge_method=S256",
        URL_SAFE_NO_PAD.encode([1; 32])
    )
}
#[test]
fn parser_rejects_duplicates_ambiguous_encoding_unsupported_flows_and_unbounded_values() {
    assert!(parse(&query()).is_ok());
    assert!(parse(&(query() + "&unknown=ignored")).is_ok());
    for extra in [
        "&client_id=other",
        "&nonce=a&nonce=b",
        "&code_challenge_method=plain",
        "&request_uri=https://evil.example",
        "&request=x",
        "&prompt=none+login",
        "&prompt=select_account",
        "&max_age=-1",
        "&max_age=1.5",
        "&state=%FF",
        "&state=%ZZ",
        "&state=%",
        "&state=%00",
    ] {
        assert!(parse(&(query() + extra)).is_err(), "{extra}");
    }
    for (old, new) in [
        ("response_type=code", "response_type=token"),
        ("scope=openid", "scope=openid+openid"),
        ("S256", "plain"),
        ("code_challenge=", "code_challenge=X"),
        (
            "client_id=00000000-0000-0000-0000-000000000020",
            "client_id=00000000000000000000000000000020",
        ),
    ] {
        assert!(parse(&query().replace(old, new)).is_err());
    }
    assert!(parse(&"x".repeat(8193)).is_err());
    assert!(parse("").is_err());
    let request =
        parse(&(query() + "&prompt=login+consent&max_age=0&nonce=n&state=x%26y%3Dz")).unwrap();
    assert_eq!(request.prompt, Prompt::LoginConsent);
    assert_eq!(request.state.as_deref(), Some("x&y=z"));
}
