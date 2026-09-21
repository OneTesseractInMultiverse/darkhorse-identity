use super::*;
fn fields(input: Input) -> Result<Fields, Error> {
    Fields::new(input, &["CR", "US", "GB", "AX", "ZW"])
}
fn input() -> Input {
    Input {
        first_name: " María ".into(),
        second_name: " José ".into(),
        last_name: "de la Cruz".into(),
        second_last_name: "Guzmán".into(),
        country: "CR".into(),
        bio: "🦀".repeat(2000),
        phone: None,
    }
}
#[test]
fn international_names_and_bio_keep_components_and_scalar_limits() {
    let profile = fields(input()).unwrap();
    assert_eq!(profile.first_name(), "María");
    assert_eq!(profile.last_name(), "de la Cruz");
    assert_eq!(profile.second_name(), Some("José"));
    assert_eq!(profile.second_last_name(), Some("Guzmán"));
    assert_eq!(profile.bio().unwrap().len(), 8000);
    let mut value = input();
    value.bio.push('x');
    assert_eq!(fields(value), Err(Error::Invalid));
    for name in ["", "\n", &"x".repeat(101), "a\0b"] {
        let mut value = input();
        value.first_name = name.into();
        assert_eq!(fields(value), Err(Error::Invalid));
    }
    let mut value = input();
    value.second_name = " ".into();
    value.second_last_name.clear();
    value.country.clear();
    value.bio = "first\nsecond\tthird".into();
    let profile = fields(value).unwrap();
    assert_eq!(profile.second_name(), None);
    assert_eq!(profile.country(), None);
    for bio in ["bad\0text", "bad\rtext", "bad\u{7f}text"] {
        let mut value = input();
        value.bio = bio.into();
        assert_eq!(fields(value), Err(Error::Invalid));
    }
}
#[test]
fn countries_are_canonical_codes_and_phones_are_bounded_e164_values() {
    for country in ["CR", "US", "GB", "AX", "ZW"] {
        let mut value = input();
        value.country = country.into();
        assert!(fields(value).is_ok());
    }
    for country in ["cr", "XX", "ZZ", " CR ", "Costa Rica", "XK"] {
        let mut value = input();
        value.country = country.into();
        assert_eq!(fields(value), Err(Error::Invalid));
    }
    let phone = Phone::new("506", "88887777").unwrap();
    assert_eq!(phone.e164(), "+50688887777");
    assert_eq!(phone.calling_code(), "506");
    assert_eq!(phone.national_number(), "88887777");
    for (code, number) in [
        ("", "1"),
        ("0", "123456"),
        ("+1", "123456"),
        ("1234", "123456"),
        ("1", ""),
        ("1", "a"),
        ("1", "123456789012345"),
    ] {
        assert_eq!(Phone::new(code, number), Err(Error::Invalid));
    }
    let mut value = input();
    value.phone = Some(phone);
    assert!(fields(value).unwrap().phone().is_some());
}
#[test]
fn owner_or_administrator_only_and_fresh_authentication_are_explicit() {
    let one = PrincipalId::from_u128(1).unwrap();
    let two = PrincipalId::from_u128(2).unwrap();
    assert_eq!(authorize(one, one, false), Ok(()));
    assert_eq!(authorize(one, two, false), Err(Error::Forbidden));
    assert_eq!(authorize(one, two, true), Ok(()));
    assert_eq!(recent(1, 300000), Ok(()));
    assert_eq!(recent(1, 300001), Err(Error::RecentAuthentication));
    assert_eq!(recent(10, 9), Err(Error::RecentAuthentication));
    assert_eq!(revision(0, 0), Ok(1));
    assert_eq!(revision(0, 1), Err(Error::Conflict));
    assert_eq!(
        revision(i64::MAX as u64, i64::MAX as u64),
        Err(Error::Conflict)
    );
}
