use super::*;
use crate::identity::{ApplicationId, ClientId};

fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(1).unwrap(),
        client: ClientId::from_u128(2).unwrap(),
    }
}

#[test]
fn request_accepts_the_supported_overlap_range_and_normalizes_reason() {
    for overlap_seconds in [0, 1, 300] {
        let request = Request::new(target(), 0, overlap_seconds, " Planned rotation ").unwrap();
        assert_eq!(request.target(), target());
        assert_eq!(request.revision(), 0);
        assert_eq!(request.overlap_seconds(), overlap_seconds);
        assert_eq!(request.reason(), "Planned rotation");
    }
}

#[test]
fn request_rejects_unrepresentable_revision_excessive_overlap_and_invalid_reason() {
    for (revision, overlap_seconds, reason) in [
        (i64::MAX as u64, 0, "Reason"),
        (0, 301, "Reason"),
        (0, u16::MAX, "Reason"),
        (0, 0, ""),
        (0, 0, "a\nb"),
        (0, 0, "a\u{202e}b"),
    ] {
        assert_eq!(
            Request::new(target(), revision, overlap_seconds, reason),
            Err(Error::Invalid)
        );
    }
}
