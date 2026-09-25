use super::*;
fn item(value: u128) -> Metadata {
    Metadata {
        id: ClientSecretId::from_u128(value).unwrap(),
        created_ms: 10,
        expires_ms: None,
        retired: false,
    }
}
#[test]
fn continuation_uses_last_returned_row_and_does_not_skip_the_probe() {
    let p = page(7, 20, vec![item(1), item(2), item(3)], 2).unwrap();
    assert_eq!(p.items, vec![item(1), item(2)]);
    assert_eq!(p.next, Some(item(2).id));
    assert_eq!(p.revision, 7);
    assert_eq!(p.observed_ms, 20);
    for items in [vec![], vec![item(3)], vec![item(3), item(4)]] {
        assert_eq!(page(7, 20, items, 2).unwrap().next, None);
    }
}
#[test]
fn corrupt_or_unbounded_pages_fail_closed() {
    for limit in [0, 26, u16::MAX] {
        assert_eq!(page(0, 20, vec![], limit), Err(Error::Unavailable));
    }
    assert_eq!(page(-1, 20, vec![], 1), Err(Error::Unavailable));
    assert_eq!(
        page(0, 20, vec![item(1), item(2), item(3)], 1),
        Err(Error::Unavailable)
    );
}
