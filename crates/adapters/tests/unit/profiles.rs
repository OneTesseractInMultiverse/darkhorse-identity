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

#[test]
fn calling_code_must_match_the_international_number_boundary() {
    // These pairs concatenate to valid numbers but use the wrong dropdown code.
    for (code, number) in [
        ("50", "688887777"),
        ("12", "025550123"),
        ("390", "236618300"),
    ] {
        let mut value = input();
        value.phone = Some(Phone::new(code, number).unwrap());
        assert_eq!(prepare(value), Err(Error::Invalid));
    }
}

#[test]
fn international_contacts_preserve_canonical_digits_and_optional_country() {
    for (code, number) in [
        ("1", "2025550123"),
        ("1", "4165550123"),
        ("7", "9123456789"),
        ("33", "142764978"),
        ("39", "0236618300"),
        ("44", "2070313000"),
        ("49", "30123456"),
        ("55", "11961234567"),
        ("61", "412345678"),
        ("81", "9012345678"),
        ("91", "9123456789"),
        ("370", "80012345"),
        ("506", "88887777"),
        ("800", "12345678"),
    ] {
        let mut value = input();
        value.country.clear();
        let phone = Phone::new(code, number).unwrap();
        value.phone = Some(phone.clone());
        let fields = prepare(value).unwrap();
        assert_eq!(fields.phone(), Some(&phone));
        assert_eq!(fields.country(), None);
        assert!(calling_codes().contains(&code.parse().unwrap()));
    }
}

#[test]
fn formatted_extensions_and_national_dialling_prefixes_are_rejected() {
    for number in [
        "2025550123x42",
        "2025550123;ext=42",
        "202-555-0123",
        "２０２５５５０１２３",
        "2025550123\n",
        "2025550123\0",
        "1(202)5550123",
    ] {
        assert_eq!(Phone::new("1", number), Err(Error::Invalid));
    }
    for (code, number) in [
        ("1", "12025550123"),
        ("44", "02070313000"),
        ("33", "0142764978"),
        ("506", "00000000"),
        ("353", "22123450"),
        ("999", "123456789"),
    ] {
        let mut value = input();
        value.phone = Some(Phone::new(code, number).unwrap());
        assert_eq!(prepare(value), Err(Error::Invalid));
    }
}

#[test]
fn stored_contacts_keep_structural_validation_without_current_numbering_rules() {
    let mut value = input();
    value.phone = Some(Phone::new("353", "22123450").unwrap());
    assert_eq!(prepare(value.clone()), Err(Error::Invalid));
    assert_eq!(
        stored_fields(value.clone())
            .unwrap()
            .phone()
            .unwrap()
            .e164(),
        "+35322123450"
    );
    value.first_name.clear();
    assert_eq!(stored_fields(value), Err(Error::Invalid));
}
