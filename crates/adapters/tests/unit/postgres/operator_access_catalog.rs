use super::*;
use darkhorse_domain::{admin_catalog::Change, registration::RegistrationError};
use std::{
    num::NonZeroU128,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct Material(AtomicUsize);
impl Entropy for Material {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(NonZeroU128::new(17).unwrap())
    }
    fn secret(&self) -> Result<darkhorse_application::registration::NewSecret, RegistrationError> {
        panic!("access-catalog mutations never issue secrets")
    }
}

#[test]
fn entropy_is_used_only_for_new_catalog_definitions() {
    let material = Material::default();
    let create = Change::CreateRole {
        name: darkhorse_domain::registration::Label::new("Reader").unwrap(),
        application: None,
    };
    assert_eq!(
        new_identifier(&create, &material).unwrap().unwrap().get(),
        17
    );
    assert_eq!(material.0.load(Ordering::SeqCst), 1);

    let retire =
        Change::RetireCapability(darkhorse_domain::identity::CapabilityId::from_u128(2).unwrap());
    assert_eq!(new_identifier(&retire, &material).unwrap(), None);
    assert_eq!(material.0.load(Ordering::SeqCst), 1);
}

#[test]
fn registration_failures_map_to_fixed_operator_outcomes() {
    for (source, expected) in [
        (RegistrationError::Invalid, Error::Invalid),
        (RegistrationError::NotFound, Error::NotFound),
        (RegistrationError::Conflict, Error::Conflict),
        (RegistrationError::Unauthorized, Error::Denied),
        (RegistrationError::Forbidden, Error::Denied),
        (
            RegistrationError::RecentAuthenticationRequired,
            Error::Denied,
        ),
        (RegistrationError::Unavailable, Error::Unavailable),
    ] {
        assert_eq!(registration_error(source), expected);
    }
}
