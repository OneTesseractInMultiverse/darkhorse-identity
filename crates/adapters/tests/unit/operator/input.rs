use super::*;

const VALID: &[u8] = br#"{"email":"admin@example.com","first_name":"Ada","last_name":"Lovelace","password":" test-only passphrase "}"#;

#[test]
fn bounded_input_rejects_unknown_fields_and_preserves_password_bytes() {
    let input = read_json(VALID).unwrap();
    assert_eq!(input.password, " test-only passphrase ");
    for input in [b"{}".as_slice(),b"not json",br#"{"email":"a@b.com","first_name":"A","last_name":"B","password":"test-only phrase","administrator":true}"#] { assert!(parse(input).is_err()); }
    assert!(read_json(vec![b' '; INPUT_LIMIT + 1].as_slice()).is_err());
    assert!(parse(vec![b' '; INPUT_LIMIT].as_slice()).is_err());
    let error = read_json(BrokenReader).err().unwrap();
    assert_eq!(error, "Cannot read bootstrap input.");
}

struct BrokenReader;
impl std::io::Read for BrokenReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("test-only read failure"))
    }
}
