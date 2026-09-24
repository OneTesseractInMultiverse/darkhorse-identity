use super::*;
use serde_json::json;
fn value() -> serde_json::Value {
    json!({"authentication":{"email":"admin@example.com","password":"private-marker","reason":"Approved change"},
    "client":{"name":"Portal","active":false,"refresh_tokens":true,"redirect_uris":["https://portal.example/cb"],"resource_ids":[],"scope_ids":[],"token_endpoint_auth_method":"client_secret_basic"}})
}
#[test]
fn protected_client_document_is_bounded_strict_and_keeps_credentials_out_of_diagnostics() {
    let source = value().to_string();
    let input = read(source.as_bytes()).unwrap();
    assert_eq!(input.authentication.email, "admin@example.com");
    assert!(input.client.complete_spec().unwrap().refresh_tokens);
    for bytes in [
        vec![b' '; INPUT_LIMIT + 1],
        vec![255],
        b"private-marker".to_vec(),
        b"{}".to_vec(),
    ] {
        assert!(
            !read(bytes.as_slice())
                .err()
                .unwrap()
                .contains("private-marker")
        );
    }
    for path in [
        vec!["unknown"],
        vec!["authentication", "unknown"],
        vec!["client", "unknown"],
    ] {
        let mut v = value();
        if path.len() == 1 {
            v[path[0]] = true.into();
        } else {
            v[path[0]][path[1]] = true.into();
        }
        assert!(read(v.to_string().as_bytes()).is_err());
    }
    let duplicate = source.replace("\"active\":false", "\"active\":true,\"active\":false");
    assert_ne!(duplicate, source);
    assert!(read(duplicate.as_bytes()).is_err());
    let padded = format!("{source}{}", " ".repeat(INPUT_LIMIT - source.len()));
    assert!(read(padded.as_bytes()).is_ok());
    assert!(read(Failed).is_err());
}

struct Failed;
impl std::io::Read for Failed {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("private-marker"))
    }
}
