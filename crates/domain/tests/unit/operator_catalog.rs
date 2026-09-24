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
