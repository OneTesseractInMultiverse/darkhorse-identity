use super::*;
use crate::postgres::operator_client_rotation::RotatedSecret;
use darkhorse_domain::{identity::ClientSecretId, operator_accounts::Error};
use zeroize::Zeroizing;

#[test]
fn audit_records_written_and_denied_results_without_secret_values() {
    let written = RotatedSecret {
        revision: 8,
        secret_id: ClientSecretId::from_u128(3).unwrap(),
        secret: Zeroizing::new("sensitive-secret-marker".to_owned()),
    };
    let record = result(7, &Ok(written)).unwrap();
    assert_eq!(record.0, "written");
    assert_eq!(record.1, Some(8));
    assert_eq!(record.2, Some(Uuid::from_u128(3)));

    for (error, expected) in [
        (Error::Denied, "denied"),
        (Error::Invalid, "invalid"),
        (Error::NotFound, "not_found"),
        (Error::Conflict, "conflict"),
    ] {
        assert_eq!(result(7, &Err(error)).unwrap(), (expected, None, None));
    }
    assert!(result(6, &Err(Error::Unavailable)).is_err());
    assert!(result(7, &Err(Error::Uncertain)).is_err());
    assert!(result(7, &Err(Error::Limited { retry_after_ms: 1 })).is_err());
}

#[test]
fn audit_refuses_nonincrementing_outcomes() {
    let written = RotatedSecret {
        revision: 9,
        secret_id: ClientSecretId::from_u128(3).unwrap(),
        secret: Zeroizing::new("private".to_owned()),
    };
    assert!(result(7, &Ok(written)).is_err());
}
