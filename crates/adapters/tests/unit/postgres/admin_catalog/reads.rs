use super::*;
#[test]
fn catalog_pages_bind_literal_prefixes_and_reject_unsupported_status_filters() {
    assert_eq!(prefix("%_\\"), "\\%\\_\\\\%");
    let q = Query {
        search: String::new(),
        active: Some(true),
        after: None,
        limit: 25,
    };
    assert!(statement(List::Applications, &q).is_ok());
    assert!(statement(List::Roles(None), &q).is_err());
    let page = page(vec![], 25, 9);
    assert!(page.items.is_empty());
    assert!(page.next.is_none());
    assert_eq!(page.policy_revision, 9);
}
