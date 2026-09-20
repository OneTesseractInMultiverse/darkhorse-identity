use super::*;
#[test]
fn bounded_queries_and_names_preserve_literal_search() {
    let mut q = Query {
        status: None,
        search: "%_ O'Connor".into(),
        after: None,
        limit: 100,
    };
    assert_eq!(q.validate(), Ok(()));
    for limit in [0, 101] {
        q.limit = limit;
        assert_eq!(q.validate(), Err(Error::Invalid));
    }
    q.limit = 25;
    for search in [" padded".into(), "line\nbreak".into(), "x".repeat(101)] {
        q.search = search;
        assert_eq!(q.validate(), Err(Error::Invalid));
    }
    assert_eq!(
        Names::new(" Ada ", "Lovelace").unwrap(),
        Names {
            first: "Ada".into(),
            last: "Lovelace".into()
        }
    );
    assert_eq!(Names::new("", "Lovelace"), Err(Error::Invalid));
    assert_eq!(Names::new("Ada", "\n"), Err(Error::Invalid));
    assert_eq!(revision(4, 4), Ok(5));
    assert_eq!(revision(4, 3), Err(Error::Conflict));
    assert_eq!(
        revision(i64::MAX as u64, i64::MAX as u64),
        Err(Error::Conflict)
    );
}
#[test]
fn role_changes_require_current_policy_and_an_active_application_for_grants() {
    for current in [false, true] {
        for desired in [false, true] {
            assert_eq!(
                role_change(5, 5, true, current, desired),
                Ok(current != desired)
            );
            assert_eq!(
                role_change(5, 4, true, current, desired),
                Err(Error::Conflict)
            );
            assert_eq!(
                role_change(5, 5, false, current, desired),
                if desired {
                    Err(Error::Invalid)
                } else {
                    Ok(current)
                }
            );
        }
    }
}
