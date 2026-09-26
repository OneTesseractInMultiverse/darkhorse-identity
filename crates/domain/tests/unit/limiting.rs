use super::*;

fn budget(key: u8, limit: u32, window: u32) -> Budget {
    Budget {
        key: [key; 32],
        rule: BudgetRule::new(limit, window).unwrap(),
    }
}
fn request() -> Attempt {
    Attempt::new(vec![budget(1, 2, 1000), budget(2, 3, 2000)]).unwrap()
}

#[test]
fn validates_bounded_rules_and_distinct_nonempty_budgets() {
    for (limit, window) in [(0, 1000), (1_000_001, 1000), (1, 999), (1, 900_001)] {
        assert_eq!(
            BudgetRule::new(limit, window),
            Err(LimitError::InvalidInput)
        );
    }
    let maximum = BudgetRule::new(1_000_000, 900_000).unwrap();
    assert_eq!(maximum.limit(), 1_000_000);
    assert_eq!(maximum.window_ms(), 900_000);
    assert_eq!(Attempt::new(vec![]), Err(LimitError::InvalidInput));
    assert_eq!(
        Attempt::new(vec![budget(1, 1, 1000); 5]),
        Err(LimitError::InvalidInput)
    );
    assert_eq!(
        Attempt::new(vec![budget(1, 1, 1000); 2]),
        Err(LimitError::InvalidInput)
    );
    assert_eq!(
        Attempt::new((0..4).map(|i| budget(i, 1, 1000)).collect())
            .unwrap()
            .budgets()
            .len(),
        4
    );
}

#[test]
fn all_budgets_charge_together_and_expiry_is_inclusive() {
    let request = request();
    let Plan::Charge(first) = plan(&request, &[None, None], 10).unwrap() else {
        panic!()
    };
    assert_eq!(first.iter().map(|c| c.used).collect::<Vec<_>>(), vec![1, 1]);
    assert_eq!(first[0].started_ms, 10);
    let Plan::Charge(second) = plan(
        &request,
        &first.iter().copied().map(Some).collect::<Vec<_>>(),
        11,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(second[0].used, 2);
    assert_eq!(second[0].last_ms, 11);
    assert_eq!(
        plan(
            &request,
            &[
                Some(second[0]),
                Some(Counter {
                    used: 3,
                    ..second[1]
                })
            ],
            12
        ),
        Ok(Plan::Limited {
            retry_after_ms: 1998
        })
    );
    assert_eq!(
        plan(
            &request,
            &second.iter().copied().map(Some).collect::<Vec<_>>(),
            1009
        ),
        Ok(Plan::Limited { retry_after_ms: 1 })
    );
    let Plan::Charge(reset) = plan(
        &request,
        &second.iter().copied().map(Some).collect::<Vec<_>>(),
        1010,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(reset[0].used, 1);
    assert_eq!(reset[0].started_ms, 1010);
    assert_eq!(reset[1].used, 3);
    assert_eq!(reset[1].started_ms, 10);
    assert_eq!(
        plan(
            &request,
            &reset.iter().copied().map(Some).collect::<Vec<_>>(),
            1011
        ),
        Ok(Plan::Limited {
            retry_after_ms: 999
        })
    );
    assert_eq!(
        plan(
            &request,
            &[
                Some(Counter {
                    used: 2,
                    ..reset[0]
                }),
                Some(reset[1])
            ],
            1011
        ),
        Ok(Plan::Limited {
            retry_after_ms: 999
        })
    );
}

#[test]
fn malformed_state_policy_changes_and_clock_rollback_fail_closed() {
    let request = request();
    assert_eq!(plan(&request, &[], 0), Err(LimitError::InvalidInput));
    assert_eq!(
        plan(&request, &[None, None], MAX_TIME + 1),
        Err(LimitError::InvalidInput)
    );
    let good = Counter {
        rule: request.budgets()[0].rule,
        used: 1,
        started_ms: 10,
        last_ms: 11,
    };
    for counter in [
        Counter { used: 0, ..good },
        Counter { used: 3, ..good },
        Counter {
            started_ms: 12,
            ..good
        },
        Counter {
            last_ms: 1010,
            ..good
        },
        Counter {
            rule: BudgetRule::new(3, 1000).unwrap(),
            ..good
        },
    ] {
        assert_eq!(
            plan(&request, &[Some(counter), None], 12),
            Err(LimitError::UnsafeState)
        );
    }
    assert_eq!(
        plan(&request, &[Some(good), None], 10),
        Err(LimitError::ClockRollback)
    );
    assert_eq!(
        plan(&request, &[None, None], MAX_TIME),
        Err(LimitError::InvalidInput)
    );
    for start in [MAX_TIME, u64::MAX] {
        assert_eq!(
            plan(
                &request,
                &[
                    Some(Counter {
                        started_ms: start,
                        last_ms: start,
                        ..good
                    }),
                    None
                ],
                12
            ),
            Err(LimitError::UnsafeState)
        );
    }
}

#[test]
fn exhaustive_small_budgets_never_admit_more_than_the_smallest_remaining_budget() {
    for left in 1..=4 {
        for right in 1..=4 {
            let request =
                Attempt::new(vec![budget(1, left, 1000), budget(2, right, 1000)]).unwrap();
            let mut state = vec![None, None];
            let mut accepted = 0;
            for _ in 0..8 {
                match plan(&request, &state, 50).unwrap() {
                    Plan::Charge(next) => {
                        accepted += 1;
                        state = next.into_iter().map(Some).collect();
                    }
                    Plan::Limited { .. } => {}
                }
            }
            assert_eq!(accepted, left.min(right));
        }
    }
}

#[test]
fn wire_integer_ceiling_accepts_the_last_complete_window_and_rejects_overflow() {
    // These literal boundaries remain independent of the implementation constant.
    let request = Attempt::new(vec![budget(1, 1, 1000)]).unwrap();
    let Plan::Charge(counters) = plan(&request, &[None], 9_007_199_254_739_991).unwrap() else {
        panic!()
    };
    assert_eq!(
        plan(&request, &[Some(counters[0])], 9_007_199_254_740_990),
        Ok(Plan::Limited { retry_after_ms: 1 })
    );
    assert_eq!(
        plan(&request, &[None], 9_007_199_254_739_992),
        Err(LimitError::InvalidInput)
    );
    assert_eq!(
        plan(&request, &[None], u64::MAX),
        Err(LimitError::InvalidInput)
    );
}

#[test]
fn a_shared_budget_binds_related_policy_even_after_its_window_expires() {
    let old = BudgetRule::new(100, 1000).unwrap().bound_to(10);
    let changed = BudgetRule::new(100, 1000).unwrap().bound_to(20);
    let request = Attempt::new(vec![Budget {
        key: [1; 32],
        rule: changed,
    }])
    .unwrap();
    let counter = Counter {
        rule: old,
        used: 1,
        started_ms: 0,
        last_ms: 0,
    };
    for now in [1, 1000, 2000] {
        assert_eq!(
            plan(&request, &[Some(counter)], now),
            Err(LimitError::UnsafeState)
        );
    }
    assert_eq!(old.binding(), 10);
    assert_eq!(BudgetRule::new(100, 1000).unwrap().binding(), 0);
}
