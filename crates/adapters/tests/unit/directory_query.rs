use super::*;
use darkhorse_domain::AccountStatus;

#[test]
fn admin_directory_translates_bounded_filters_and_literal_prefixes() {
    let query = parse_admin("status=active&limit=25&search=Ada%20Lovelace").unwrap();
    assert_eq!(query.status, Some(AccountStatus::Active));
    assert_eq!(query.search, "Ada Lovelace");
    assert_eq!(query.limit, 25);
    assert!(query.after.is_none());
}

#[test]
fn defaults_are_bounded() {
    assert_eq!(
        parse("").unwrap(),
        DirectoryCriteria {
            status: None,
            limit: 25,
            offset: 0
        }
    );
}

#[test]
fn maps_allowed_values_into_owned_criteria() {
    assert_eq!(
        parse("status=active&limit=100&skip=1000").unwrap(),
        DirectoryCriteria {
            status: Some(AccountStatus::Active),
            limit: 100,
            offset: 1000
        }
    );
    assert_eq!(
        parse("status=inactive").unwrap().status,
        Some(AccountStatus::Inactive)
    );
    assert_eq!(
        parse("%73tatus=active").unwrap().status,
        Some(AccountStatus::Active)
    );
}

#[test]
fn rejects_unknown_operators_values_duplicates_and_unbounded_queries() {
    for raw in [
        "status",
        "email=private@example.org",
        "status!=active",
        "status=admin",
        "status=null",
        "status=in(active,inactive)",
        "status=active&status=inactive",
        "limit=1&limit=2",
        "skip=1&%73kip=2",
        "sort=status",
        "fields=",
        "limit=0",
        "limit=101",
        "limit=-1",
        "skip=1001",
        "status=%ZZ",
    ] {
        assert_eq!(parse(raw), Err(InvalidQuery), "{raw}");
    }
    assert_eq!(parse(&"x".repeat(2049)), Err(InvalidQuery));
}

#[test]
fn pagination_value_limits_apply_after_decoding_without_changing_numeric_bounds() {
    for zero in ["0", "%30"] {
        let limit = format!("limit={}25", zero.repeat(30));
        assert_eq!(parse(&limit).unwrap().limit, 25);
        assert_eq!(parse_admin(&limit).unwrap().limit, 25);
        assert_eq!(parse_catalog(&limit).unwrap().limit, 25);
        let too_long = format!("limit={}25", zero.repeat(31));
        assert_eq!(parse(&too_long), Err(InvalidQuery));
        assert_eq!(parse_admin(&too_long), Err(InvalidQuery));
        assert_eq!(parse_catalog(&too_long), Err(InvalidQuery));
        assert_eq!(
            parse(&format!("skip={}1", zero.repeat(31))).unwrap().offset,
            1
        );
        assert_eq!(
            parse(&format!("skip={}1", zero.repeat(32))),
            Err(InvalidQuery)
        );
    }
}

#[test]
fn administrative_profiles_preserve_literal_search_status_and_cursor_contracts() {
    let id = "00000000-0000-0000-0000-000000000001";
    for status in ["active", "str(active)"] {
        let raw = format!("%73tatus={status}&limit=25&search=a%3E%3Db%3C%21%3D%25_%5C&after={id}");
        let user = parse_admin(&raw).unwrap();
        let catalog = parse_catalog(&raw).unwrap();
        assert_eq!(user.search, "a>=b<!=%_\\");
        assert_eq!(catalog.search, user.search);
        assert_eq!(user.status, Some(AccountStatus::Active));
        assert_eq!(catalog.active, Some(true));
        assert_eq!(user.after.unwrap().as_u128(), 1);
        assert_eq!(catalog.after.unwrap().get(), 1);
        assert_eq!((user.limit, catalog.limit), (25, 25));
    }
    let long_search = format!("search={}", "é".repeat(100));
    assert_eq!(
        parse_admin(&long_search).unwrap().search.chars().count(),
        100
    );
    assert_eq!(
        parse_catalog(&long_search).unwrap().search.chars().count(),
        100
    );
    for raw in [
        "status=active&%73tatus=inactive",
        "status=active%3E%3Dinactive",
        "status=str(active%3D%3Dprivate)",
        "status!=active",
        "status=/active/",
        "status=null",
        "status=in(active,inactive)",
        "sort=status",
        "fields=email",
        "unknown%0Aprivate=value",
        "status=%00private",
        "status=%FF",
        "limit=101",
        "limit=0",
        "skip=1",
        "after=00000000-0000-0000-0000-000000000000",
        "after=00000000000000000000000000000001",
        "after=bad",
        "after=00000000-0000-0000-0000-000000000001&after=00000000-0000-0000-0000-000000000002",
    ] {
        assert_eq!(parse_admin(raw), Err(InvalidQuery), "{raw}");
        assert_eq!(parse_catalog(raw), Err(InvalidQuery), "{raw}");
    }
    for raw in ["x".repeat(2049), format!("search={}", "x".repeat(101))] {
        assert_eq!(parse_admin(&raw), Err(InvalidQuery));
        assert_eq!(parse_catalog(&raw), Err(InvalidQuery));
    }
}
