use super::*;
use darkhorse_domain::identity::RoleId;

#[test]
fn catalog_audit_records_bounded_command_state_and_scoped_target_ids() {
    let role = RoleId::from_u128(4).unwrap();
    let capability = darkhorse_domain::identity::CapabilityId::from_u128(5).unwrap();
    let change = Change::RoleCapability {
        role,
        capability,
        granted: false,
    };
    let request = MutationRequest::new(7, change, "Remove unused capability").unwrap();
    let metadata = values(request.change(), &Err(Error::Conflict));
    assert_eq!(command(request.change()), "role.capability");
    assert_eq!(metadata.target, Some(uuid(role.as_u128())));
    assert_eq!(metadata.related, Some(uuid(capability.as_u128())));
    assert_eq!(metadata.application, None);
    assert_eq!(metadata.requested_state, Some(false));
    assert_eq!(result(&Err(Error::Conflict), 7), Ok("conflict"));
    assert_eq!(result(&Err(Error::Denied), 7), Ok("denied"));
    let written = Written {
        target: Target::Role(role),
        policy_revision: 9,
        principal_revision: None,
    };
    assert_eq!(result(&Ok(written), 7), Ok("changed"));
    let unchanged = Written {
        target: Target::Role(role),
        policy_revision: 7,
        principal_revision: None,
    };
    assert_eq!(result(&Ok(unchanged), 7), Ok("unchanged"));
}

#[test]
fn uncertain_and_infrastructure_errors_are_never_audited_as_definite() {
    for error in [
        Error::Limited { retry_after_ms: 1 },
        Error::Unavailable,
        Error::Uncertain,
    ] {
        assert_eq!(result(&Err(error), 0), Err(Error::Unavailable));
    }
}
