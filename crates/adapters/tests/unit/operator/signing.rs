use super::*;
#[test]
fn input_bounds_and_public_status_redact_private_material() {
    assert!(read_import(&b""[..]).is_err());
    assert!(read_import(&vec![0; 8193][..]).is_err());
    assert_eq!(&*read_import(&b"abc"[..]).unwrap(), b"abc");
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("private input"))
        }
    }
    assert!(read_import(Broken).is_err());
    for phase in [
        Phase::Staged,
        Phase::Active,
        Phase::Retiring,
        Phase::Retired,
    ] {
        let projected = project(KeyInventory {
            revision: 2,
            keys: vec![darkhorse_application::signing::KeyRecord {
                public: darkhorse_application::signing::PublicKey {
                    kid: "public".into(),
                    n: "modulus".into(),
                    e: "AQAB".into(),
                },
                state: darkhorse_domain::signing::KeyState {
                    phase,
                    created_ms: 0,
                    activated_ms: Some(60_000),
                    verify_until_ms: Some(660_000),
                },
            }],
        });
        assert_eq!(projected["revision"], 2);
        assert!(projected["keys"][0].get("n").is_none());
    }
    for error in [
        KeyError::Invalid,
        KeyError::Conflict,
        KeyError::NotReady,
        KeyError::NotFound,
        KeyError::Unavailable,
    ] {
        assert!(!message(error).contains("private input"));
    }
}
