use super::*;
#[test]
fn journal_metadata_rejects_unknown_variants_and_out_of_range_times() {
    for kind in [Kind::Generate, Kind::Import, Kind::Activate, Kind::Retire] {
        assert_eq!(parse_kind(kind_name(kind)), Ok(kind));
    }
    assert!(parse_kind("unknown").is_err());
    for (name, phase) in [
        ("staged", Phase::Staged),
        ("active", Phase::Active),
        ("retiring", Phase::Retiring),
        ("retired", Phase::Retired),
    ] {
        assert_eq!(parse_phase(name), Ok(phase));
    }
    assert!(parse_phase("unknown").is_err());
    assert_eq!(integer(i64::MAX as u64), Ok(i64::MAX));
    assert!(integer(u64::MAX).is_err());
    assert_eq!(time(0), Ok(0));
    assert!(time(-1).is_err());
}
#[test]
fn prepared_material_must_match_the_recorded_command_and_target() {
    let key = WrappedKey {
        public: darkhorse_application::signing::PublicKey {
            kid: "public".into(),
            n: "n".into(),
            e: "e".into(),
        },
        nonce: [1; 12],
        ciphertext: vec![2; 32],
    };
    for kind in [Kind::Generate, Kind::Import, Kind::Activate, Kind::Retire] {
        let mut intent = Intent {
            id: OperationId::from_u128(1).unwrap(),
            issuer: "https://issuer.example".into(),
            kid: "public".into(),
            kind,
            expected_revision: 0,
        };
        let stage = matches!(kind, Kind::Generate | Kind::Import);
        assert_eq!(validate_material(&intent, Some(&key)).is_ok(), stage);
        assert_eq!(validate_material(&intent, None).is_ok(), !stage);
        intent.kid = "different".into();
        assert!(validate_material(&intent, Some(&key)).is_err());
    }
}
