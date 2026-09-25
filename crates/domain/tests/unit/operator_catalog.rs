use super::*;
#[test]
fn catalog_pages_preserve_typed_target_and_validate_all_query_bounds() {
    let app = ApplicationId::from_u128(7).unwrap();
    let query = Query {
        search: String::new(),
        active: None,
        after: None,
        limit: 25,
    };
    for target in [Target::Applications, Target::Clients(app)] {
        for limit in [1, 25] {
            let r = Request::new(
                target,
                Query {
                    limit,
                    ..query.clone()
                },
            )
            .unwrap();
            assert_eq!(r.target(), target);
            assert_eq!(r.query().limit, limit);
        }
    }
    for limit in [0, 26, 100, u16::MAX] {
        assert_eq!(
            Request::new(
                Target::Applications,
                Query {
                    limit,
                    ..query.clone()
                }
            ),
            Err(Error::Invalid)
        );
    }
    for search in ["x\0y".into(), " trim".into(), "界".repeat(101)] {
        assert_eq!(
            Request::new(
                Target::Applications,
                Query {
                    search,
                    ..query.clone()
                }
            ),
            Err(Error::Invalid)
        );
    }
    assert!(
        Request::new(
            Target::Applications,
            Query {
                search: "界".repeat(100),
                ..query
            }
        )
        .is_ok()
    );
}

#[test]
fn access_catalog_selection_and_status_are_explicit() {
    let app = ApplicationId::from_u128(7).unwrap();
    for target in [
        Target::Resources(app),
        Target::Scopes(app),
        Target::Roles(Definitions::Application(app)),
        Target::Roles(Definitions::All),
        Target::Capabilities(Definitions::Application(app)),
        Target::Capabilities(Definitions::All),
    ] {
        for active in [None, Some(true), Some(false)] {
            let query = Query {
                search: "audit%_\\marker".into(),
                active,
                after: None,
                limit: 25,
            };
            let expected = active.is_none() || matches!(target, Target::Capabilities(_));
            assert_eq!(Request::new(target, query.clone()).is_ok(), expected);
            if expected {
                assert_eq!(Request::new(target, query.clone()).unwrap().query(), &query);
            }
        }
    }
}
