use super::*;
#[test]
fn ordered_hints_resolve_supported_regions_without_rejecting_unavailable_languages() {
    assert_eq!(ui_locales(None), Ok(None));
    assert_eq!(ui_locales(Some("es-CR es en")), Ok(Some(Locale::Spanish)));
    assert_eq!(
        ui_locales(Some("de fr en-US es")),
        Ok(Some(Locale::English))
    );
    assert_eq!(
        ui_locales(Some("ES-cr es-CR EN")),
        Ok(Some(Locale::Spanish))
    );
    assert_eq!(ui_locales(Some("de-DE fr-CA")), Ok(None));
    assert_eq!(ui_locales(Some("x-private i-klingon")), Ok(None));
    assert_eq!(
        ui_locales(Some("en-US-u-ca-gregory es")),
        Ok(Some(Locale::English))
    );
}
#[test]
fn every_hint_is_checked_even_after_a_supported_choice_and_failures_are_redacted() {
    for value in [
        "",
        " en",
        "en ",
        "en  es",
        "en\tes",
        "es\nen",
        "en bad_credential",
        "en ../../es",
        "en-%",
        "en-x",
        "en-123456789",
        "éé",
        "en\0",
        "en-ü",
    ] {
        assert_eq!(ui_locales(Some(value)), Err(InvalidLocale));
    }
    assert_eq!(ui_locales(Some(&["en"; 17].join(" "))), Err(InvalidLocale));
    assert_eq!(
        ui_locales(Some(&format!("en-x-{}", ["abcdefgh"; 7].join("-")))),
        Err(InvalidLocale)
    );
    assert_eq!(
        ui_locales(Some(&["es-ES-u-ca-gregory"; 16].join(" "))),
        Err(InvalidLocale)
    );
    assert_eq!(
        ui_locales(Some(&["en"; 16].join(" "))),
        Ok(Some(Locale::English))
    );
}
#[test]
fn malformed_private_use_subtags_do_not_bypass_the_language_list_grammar() {
    for value in ["x-a--b", "x-abcdefghij", "x--", "en x-a--b"] {
        assert_eq!(ui_locales(Some(value)), Err(InvalidLocale));
    }
    assert_eq!(ui_locales(Some("x-private en")), Ok(Some(Locale::English)));
}
