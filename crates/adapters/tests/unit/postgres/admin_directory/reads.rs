use super::*;
#[test]
fn prefixes_cannot_turn_literal_input_into_sql_wildcards() {
    assert_eq!(prefix("a%_\\'"), "a\\%\\_\\\\'%");
    let result = page(PrincipalId::from_u128(1).unwrap(), vec![], 25).unwrap();
    assert!(result.items.is_empty());
    assert!(result.next.is_none());
}
