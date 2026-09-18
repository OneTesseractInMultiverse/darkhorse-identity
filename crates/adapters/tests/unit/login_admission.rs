use super::*;
#[test]
fn budgets_are_shared_separate_keyed_and_fixed_by_server() {
    let a = attempt(&[1; 32], "one@example.com").unwrap();
    let b = attempt(&[1; 32], "two@example.com").unwrap();
    let c = attempt(&[2; 32], "one@example.com").unwrap();
    assert_eq!(a, attempt(&[1; 32], "one@example.com").unwrap());
    assert_eq!(a.budgets()[0], b.budgets()[0]);
    assert_ne!(a.budgets()[1].key, b.budgets()[1].key);
    assert_ne!(a.budgets()[1].key, a.budgets()[2].key);
    for i in 0..3 {
        assert_ne!(a.budgets()[i].key, c.budgets()[i].key);
    }
    assert_eq!(
        a.budgets()
            .iter()
            .map(|b| (b.rule.limit(), b.rule.window_ms()))
            .collect::<Vec<_>>(),
        [(120, 60000), (5, 60000), (30, 900000)]
    );
    assert_eq!(outcome(Admission::Allowed), Ok(()));
    assert_eq!(
        outcome(Admission::Limited { retry_after_ms: 42 }),
        Err(AuthError::Limited { retry_after_ms: 42 })
    );
}
