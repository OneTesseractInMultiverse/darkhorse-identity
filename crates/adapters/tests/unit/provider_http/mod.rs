use super::*;
#[test]
fn request_cookie_uses_a_separate_digest_namespace() {
    let value = "01".repeat(32);
    assert_ne!(
        handle_digest(&value).unwrap(),
        session_secret::digest(&value).unwrap()
    );
    assert!(handle_digest("bad").is_err());
    let (one, _) = handle().unwrap();
    let (two, _) = handle().unwrap();
    assert_ne!(one, two);
}
