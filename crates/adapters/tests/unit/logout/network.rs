use super::*;
#[test]
fn public_routes_reject_special_ranges_and_mixed_dns_answers() {
    let policy = Policy::parse("{}").unwrap();
    let target = super::super::destination("https://app.example/logout").unwrap();
    let good = "8.8.8.8".parse().unwrap();
    assert!(policy.addresses(&target, &[good]).is_ok());
    for text in [
        "0.0.0.1",
        "10.0.0.1",
        "100.64.0.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.16.0.1",
        "192.0.0.1",
        "192.0.2.1",
        "192.88.99.1",
        "192.168.1.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "240.1.1.1",
        "::",
        "::1",
        "::ffff:8.8.8.8",
        "64:ff9b::808:808",
        "2001:db8::1",
        "2002::1",
        "3fff::1",
        "fc00::1",
        "fe80::1",
        "ff02::1",
    ] {
        let bad = text.parse().unwrap();
        assert!(policy.addresses(&target, &[bad]).is_err(), "{text}");
        assert!(policy.addresses(&target, &[good, bad]).is_err(), "{text}");
    }
    assert!(policy.addresses(&target, &[]).is_err());
    assert!(policy.addresses(&target, &[good; 17]).is_err());
    assert!(
        policy
            .addresses(&target, &["2606:4700:4700::1111".parse().unwrap()])
            .is_ok()
    );
}
#[test]
fn private_exceptions_bind_exact_origins_ports_and_addresses() {
    let policy =
        Policy::parse(r#"{"https://internal.example:8443":["10.1.2.3","fd00::1"]}"#).unwrap();
    let ip = "10.1.2.3".parse().unwrap();
    let target = super::super::destination("https://internal.example:8443/logout").unwrap();
    assert!(policy.addresses(&target, &[ip]).is_ok());
    for url in [
        "https://internal.example/logout",
        "https://sub.internal.example:8443/logout",
        "https://other.example:8443/logout",
    ] {
        assert!(
            policy
                .addresses(&super::super::destination(url).unwrap(), &[ip])
                .is_err()
        );
    }
    assert!(
        policy
            .addresses(&target, &["10.1.2.4".parse().unwrap()])
            .is_err()
    );
    assert!(
        policy
            .addresses(&target, &["8.8.8.8".parse().unwrap()])
            .is_err()
    );
    for input in [
        r#"{"http://internal.example":["10.1.2.3"]}"#,
        r#"{"https://internal.example/path":["10.1.2.3"]}"#,
        r#"{"https://internal.example":["169.254.169.254"]}"#,
        r#"{"https://internal.example":["::ffff:127.0.0.1"]}"#,
        r#"{"https://internal.example":[]}"#,
    ] {
        assert!(Policy::parse(input).is_err(), "{input}");
    }
}
