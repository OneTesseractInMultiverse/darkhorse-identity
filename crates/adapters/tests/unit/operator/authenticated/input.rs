use super::*;
#[test]
fn protected_input_is_bounded_strict_and_does_not_echo_secrets() {
    let input = read(
        &br#"{"email":"admin@example.com","password":"marker secret","reason":"INC-123"}"#[..],
    )
    .unwrap();
    assert_eq!(input.password, "marker secret");
    assert_eq!(input.reason.as_deref(), Some("INC-123"));
    for bytes in [
        br#"{"email":"a","password":"marker secret","actor":"root"}"#.as_slice(),
        br#"{"email":"a","password":"first","password":"marker secret"}"#,
        b"not-json",
        &[255],
    ] {
        assert!(!parse(bytes).err().unwrap().contains("marker"));
    }
    assert!(parse(&vec![b' '; 16_385]).is_err());
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }
    assert!(read(Broken).is_err());
}
