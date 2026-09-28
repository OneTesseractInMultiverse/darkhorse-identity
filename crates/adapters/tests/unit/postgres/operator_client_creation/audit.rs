use super::*;

#[test]
fn unsupported_results_never_become_definite_audit_outcomes() {
    for error in [
        Error::Unavailable,
        Error::Uncertain,
        Error::Limited { retry_after_ms: 1 },
        Error::PolicyRejected,
    ] {
        assert_eq!(result(&Err(error)), Err(Error::Unavailable));
    }
    for (error, expected) in [
        (Error::Denied, "denied"),
        (Error::Invalid, "invalid"),
        (Error::NotFound, "not_found"),
        (Error::Conflict, "conflict"),
    ] {
        assert_eq!(result(&Err(error)), Ok(expected));
    }
}
