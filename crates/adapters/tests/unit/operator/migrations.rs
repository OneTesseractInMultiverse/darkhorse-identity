use super::*;
use crate::postgres::migrations::Step;
#[test]
fn historical_completion_does_not_imply_current_history_matches() {
    let id = OperationId::from_u128(1).unwrap();
    let record = Inspection {
        database_role: "database_owner".into(),
        prepared_ms: 1,
        completed_ms: Some(2),
        database_ms: 3,
        steps: vec![Step {
            version: 1,
            checksum: "ab".into(),
            already_applied: false,
            completed_ms: Some(2),
            current_matches: false,
        }],
    };
    let data = project(id, record);
    assert_eq!(data["recorded_outcome"], "completed");
    assert_eq!(data["steps"][0]["current_matches"], false);
    assert_eq!(data["steps"][0]["already_applied"], false);
    for error in [Error::Incompatible, Error::Unavailable, Error::Uncertain] {
        assert_eq!(
            failure(error, id).data.unwrap()["operation_id"],
            identifier(id)
        );
    }
}
#[test]
fn pending_remains_pending_even_when_all_history_matches() {
    let id = OperationId::from_u128(1).unwrap();
    let data = project(
        id,
        Inspection {
            database_role: "owner".into(),
            prepared_ms: 1,
            completed_ms: None,
            database_ms: 2,
            steps: vec![Step {
                version: 1,
                checksum: "ab".into(),
                already_applied: true,
                completed_ms: None,
                current_matches: true,
            }],
        },
    );
    assert_eq!(data["recorded_outcome"], "pending");
    assert!(data["completed_ms"].is_null());
}

#[test]
fn maximum_report_fits_the_bounded_output_contract() {
    let steps = (1..=128)
        .map(|version| Step {
            version,
            checksum: "a".repeat(96),
            already_applied: false,
            completed_ms: Some(i64::MAX),
            current_matches: true,
        })
        .collect();
    let record = Inspection {
        database_role: "r".repeat(63),
        prepared_ms: i64::MAX,
        completed_ms: Some(i64::MAX),
        database_ms: i64::MAX,
        steps,
    };
    let output = Output::record(project(OperationId::from_u128(1).unwrap(), record));
    for format in [
        super::super::output::Format::Human,
        super::super::output::Format::Json,
    ] {
        assert!(
            super::super::output::render(&output, format).unwrap().len()
                < super::super::output::OUTPUT_LIMIT
        );
    }
}
