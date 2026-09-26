use super::*;
fn headers(values: &[String]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append(header::COOKIE, HeaderValue::from_str(value).unwrap());
    }
    headers
}
#[test]
fn each_public_reference_requires_its_own_proof_and_rejects_substitution() {
    let a = "01".repeat(32);
    let b = "02".repeat(32);
    let da = handle_digest(&a).unwrap();
    let db = handle_digest(&b).unwrap();
    let ra = session_secret::hex(&da);
    let rb = session_secret::hex(&db);
    let cookies = headers(&[format!("{}={a}; {}={b}", name(&ra), name(&rb))]);
    assert_eq!(select(&cookies, &ra).unwrap(), da);
    assert_eq!(select(&cookies, &rb).unwrap(), db);
    assert!(select(&headers(&[format!("{}={b}", name(&ra))]), &ra).is_err());
    assert!(select(&HeaderMap::new(), &ra).is_err());
    assert!(select(&cookies, &"03".repeat(32)).is_err());
    assert!(select(&cookies, "../secret").is_err());
    assert_eq!(reference(Some(&format!("request={ra}"))).unwrap(), ra);
    for query in [
        None,
        Some(""),
        Some("request=bad"),
        Some("request=en&request=es"),
        Some("other=value"),
    ] {
        assert!(reference(query).is_err());
    }
    assert!(reference(Some(&format!("request={ra}&ignored=true"))).is_err());
}
#[test]
fn capacity_and_duplicate_proofs_fail_closed_without_disclosing_cookie_values() {
    let mut values = Vec::new();
    for n in 1..=8 {
        let secret = format!("{n:02x}").repeat(32);
        let reference = session_secret::hex(&handle_digest(&secret).unwrap());
        values.push(format!("{}={secret}", name(&reference)));
    }
    assert!(admit(&headers(&values[..7])).is_ok());
    assert!(admit(&headers(&values)).is_err());
    let duplicate = headers(&[values[0].clone(), values[0].clone()]);
    assert!(admit(&duplicate).is_err());
    let reference = values[0]
        .split('=')
        .next()
        .unwrap()
        .strip_prefix(PREFIX)
        .unwrap();
    assert!(select(&duplicate, reference).is_err());
    assert!(admit(&headers(&[format!("unrelated={}", "x".repeat(4096))])).is_err());
    assert!(admit(&headers(&[format!("{PREFIX}bad=value")])).is_err());
    assert!(admit(&headers(&["ordinary=plain".into()])).is_ok());
}
#[test]
fn creating_and_expiring_a_cookie_retains_its_exact_host_security_scope() {
    let secret = "01".repeat(32);
    let reference = session_secret::hex(&handle_digest(&secret).unwrap());
    let value = cookie(&reference, &secret, 300).unwrap();
    assert_eq!(
        value.to_str().unwrap(),
        format!(
            "{}={secret}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=300",
            name(&reference)
        )
    );
    let expired = cookie(&reference, "", 0).unwrap();
    assert_eq!(
        expired.to_str().unwrap(),
        format!(
            "{}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
            name(&reference)
        )
    );
}
