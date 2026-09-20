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
