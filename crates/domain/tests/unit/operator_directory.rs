use super::*;
#[test]
fn listing_retains_shared_query_validation_and_tightens_the_page_bound() {
    let query = Query {
        search: String::new(),
        after: None,
        status: None,
        limit: 25,
    };
    for limit in [1, 25] {
        assert_eq!(
            Request::new(Query {
                limit,
                ..query.clone()
            })
            .unwrap()
            .query()
            .limit,
            limit
        );
    }
    for limit in [0, 26, 100, u16::MAX] {
        assert_eq!(
            Request::new(Query {
                limit,
                ..query.clone()
            }),
            Err(Error::Invalid)
        );
    }
    for search in [" x".to_owned(), "x\n".to_owned(), "界".repeat(101)] {
        assert_eq!(
            Request::new(Query {
                search,
                ..query.clone()
            }),
            Err(Error::Invalid)
        );
    }
    assert!(
        Request::new(Query {
            search: "界".repeat(100),
            ..query
        })
        .is_ok()
    );
}
