use super::*;

#[test]
fn uncertain_or_unavailable_work_cannot_be_recorded_as_a_definite_outcome() {
    for error in [
        Error::Uncertain,
        Error::Unavailable,
        Error::Limited { retry_after_ms: 1 },
    ] {
        assert_eq!(result(&Err(error)), Err(Error::Unavailable));
    }
}
