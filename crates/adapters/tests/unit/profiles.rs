use super::*;
fn input() -> Input {
    Input {
        first_name: "Ana".into(),
        second_name: String::new(),
        last_name: "Guzmán".into(),
        second_last_name: String::new(),
        country: "CR".into(),
        bio: String::new(),
        phone: None,
    }
}
#[test]
fn bundled_metadata_and_international_phone_validation_are_deterministic() {
    let countries = countries();
    assert_eq!(countries.len(), 249);
    assert!(countries.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(countries.contains(&("CR", "Costa Rica")));
    let codes = calling_codes();
    assert!(codes.contains(&506));
    assert!(codes.contains(&1));
    assert!(codes.windows(2).all(|w| w[0] < w[1]));
    for (code, number) in [
        ("506", "88887777"),
        ("1", "2025550123"),
        ("39", "0236618300"),
    ] {
        let mut value = input();
        value.phone = Some(Phone::new(code, number).unwrap());
        assert!(prepare(value).is_ok());
    }
    for (code, number) in [("999", "123456"), ("506", "1"), ("1", "0000000000")] {
        let mut value = input();
        value.phone = Some(Phone::new(code, number).unwrap());
        assert_eq!(prepare(value), Err(Error::Invalid));
    }
    assert!(prepare(input()).is_ok());
    let mut value = input();
    value.country = "ZZ".into();
    assert_eq!(prepare(value), Err(Error::Invalid));
}
