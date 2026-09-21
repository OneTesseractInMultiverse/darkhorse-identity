use super::*;
fn raw() -> Raw {
    Raw {
        enabled: true,
        endpoint: "https://storage.example.com".into(),
        bucket: "darkhorse-assets".into(),
        region: "us-east-1".into(),
        access: "key".into(),
        secret: "secret".into(),
        insecure: false,
    }
}
#[test]
fn only_explicit_tls_or_opted_in_loopback_storage_is_accepted() {
    assert!(validate(raw()).unwrap().is_some());
    let mut r = raw();
    r.enabled = false;
    r.endpoint.clear();
    assert!(validate(r).unwrap().is_none());
    for endpoint in [
        "http://storage.example.com",
        "https://key:secret@storage.example.com",
        "https://storage.example.com/path",
        "https://storage.example.com/?key=x",
        "https://storage.example.com/#fragment",
        "file:///tmp/bucket",
    ] {
        let mut r = raw();
        r.endpoint = endpoint.into();
        r.insecure = true;
        assert!(validate(r).is_err());
    }
    let mut r = raw();
    r.endpoint = "http://127.0.0.1:9000".into();
    assert!(validate(r).is_err());
    let mut r = raw();
    r.endpoint = "http://127.0.0.1:9000".into();
    r.insecure = true;
    assert!(validate(r).unwrap().unwrap().insecure);
    for bucket in ["-bad", "UPPER", "two", "bad/name"] {
        let mut r = raw();
        r.bucket = bucket.into();
        if bucket != "two" {
            assert!(validate(r).is_err());
        }
    }
    for field in ["access", "secret", "region"] {
        let mut r = raw();
        match field {
            "access" => r.access.clear(),
            "secret" => r.secret.clear(),
            _ => r.region.clear(),
        };
        assert!(validate(r).is_err());
    }
}
