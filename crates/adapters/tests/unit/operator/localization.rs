use super::*;
use darkhorse_domain::localization::Locale;
use envbind::MapEnvironment;

#[test]
fn configured_and_posix_languages_have_explicit_bounded_precedence() {
    for (pairs, expected) in [
        (vec![], Locale::English),
        (vec![("LANG", "es_CR.UTF-8")], Locale::Spanish),
        (vec![("LC_ALL", "C"), ("LANG", "es")], Locale::English),
        (
            vec![
                ("LC_ALL", ""),
                ("LC_MESSAGES", "es_ES.UTF-8"),
                ("LANG", "en"),
            ],
            Locale::Spanish,
        ),
        (
            vec![("LC_MESSAGES", "fr_FR.UTF-8"), ("LANG", "es")],
            Locale::English,
        ),
        (
            vec![("DARKHORSE_CLI_LOCALE", "es"), ("LC_ALL", "C")],
            Locale::Spanish,
        ),
        (vec![("LANG", "POSIX")], Locale::English),
        (vec![("LANG", "C.UTF-8")], Locale::English),
        (vec![("LANG", "es-CR")], Locale::Spanish),
        (vec![("LANG", "es\nforged")], Locale::English),
    ] {
        assert_eq!(load(MapEnvironment::from_pairs(pairs)).unwrap(), expected);
    }
    for invalid in ["", "fr", "ES", "es-CR", "../es", "es\n"] {
        let error = load(MapEnvironment::from_pairs([(
            "DARKHORSE_CLI_LOCALE",
            invalid,
        )]))
        .unwrap_err();
        assert!(!format!("{error:?}").contains(invalid) || invalid.len() < 3);
        assert_eq!(error.exit_code(), 1);
    }
    assert_eq!(
        load(MapEnvironment::from_pairs([("LANG", "es".repeat(100))])).unwrap(),
        Locale::English
    );
}
