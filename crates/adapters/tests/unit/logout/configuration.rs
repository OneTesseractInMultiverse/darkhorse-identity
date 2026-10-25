use super::*;
use envbind::EnvironmentError;
struct Source {
    allowlist: String,
    ca: String,
}
impl Environment for Source {
    fn get(&self, name: &str) -> Result<Option<String>, EnvironmentError> {
        Ok(match name {
            "DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS" => Some(self.allowlist.clone()),
            "DARKHORSE_LOGOUT_CA_PEM" => Some(self.ca.clone()),
            _ => None,
        })
    }
}
#[test]
fn invalid_allowlists_and_certificate_input_fail_startup() {
    assert!(
        load(Source {
            allowlist: "{}".into(),
            ca: String::new()
        })
        .is_ok()
    );
    assert!(
        load(Source {
            allowlist: "{}".into(),
            ca: "invalid".into()
        })
        .is_err()
    );
    assert!(
        load(Source {
            allowlist: "not-json".into(),
            ca: String::new()
        })
        .is_err()
    );
    assert!(
        load(Source {
            allowlist: " ".repeat(16385),
            ca: String::new()
        })
        .is_err()
    );
    let many = (0..17)
        .map(|n| format!(r#""https://app{n}.example":["10.0.0.1"]"#))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        load(Source {
            allowlist: format!("{{{many}}}"),
            ca: String::new()
        })
        .is_err()
    );
}
