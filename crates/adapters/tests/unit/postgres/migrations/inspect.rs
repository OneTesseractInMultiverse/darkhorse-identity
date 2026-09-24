use super::*;
fn record(count: i32) -> Record {
    Record {
        database_role: "owner".into(),
        prepared_ms: 10,
        completed_ms: Some(20),
        database_ms: 30,
        manifest_count: count,
    }
}
fn steps(count: usize) -> Vec<Step> {
    (1..=count)
        .map(|n| Step {
            version: n as i64,
            checksum: "a".repeat(96),
            already_applied: false,
            completed_ms: None,
            current_matches: false,
        })
        .collect()
}
#[test]
fn projection_rejects_missing_overflow_or_inconsistent_manifest() {
    for (declared, count) in [(0, 0), (-1, 1), (2, 1), (1, 2), (129, 129)] {
        assert!(matches!(
            project(record(declared), steps(count)),
            Err(Error::Incompatible)
        ));
    }
}
#[test]
fn projection_preserves_history_and_current_observations_within_bound() {
    for count in [1, 128] {
        let observed = project(record(count), steps(count as usize)).unwrap();
        assert_eq!(observed.completed_ms, Some(20));
        assert_eq!(observed.database_ms, 30);
        assert_eq!(observed.steps.len(), count as usize);
        assert!(!observed.steps[0].current_matches);
    }
}
