use super::*;
#[test]
fn catalog_inputs_preserve_permission_identity_and_bound_literal_queries() {
    assert_eq!(
        PermissionDefinition::new("orders.read", "Read orders")
            .unwrap()
            .key(),
        "orders.read"
    );
    assert_eq!(
        PermissionDefinition::new("read", "  Read records  ")
            .unwrap()
            .meaning(),
        "Read records"
    );
    for key in ["", "bad key", "UPPER", "orders.*"] {
        assert_eq!(
            PermissionDefinition::new(key, "Meaning"),
            Err(Error::Invalid)
        );
    }
    for meaning in ["".into(), "x".repeat(1001), "line\nbreak".into()] {
        assert_eq!(
            PermissionDefinition::new("read", &meaning),
            Err(Error::Invalid)
        );
    }
    let mut q = Query {
        search: "%_".into(),
        active: None,
        after: None,
        limit: 100,
    };
    assert_eq!(q.validate(), Ok(()));
    q.limit = 101;
    assert_eq!(q.validate(), Err(Error::Invalid));
    q.limit = 0;
    assert_eq!(q.validate(), Err(Error::Invalid));
    q.limit = 25;
    for text in [" padded".into(), "x".repeat(101), "line\nbreak".into()] {
        q.search = text;
        assert_eq!(q.validate(), Err(Error::Invalid));
    }
}
#[test]
fn shared_role_grants_require_every_application_binding() {
    let a = ApplicationId::from_u128(1).unwrap();
    let b = ApplicationId::from_u128(2).unwrap();
    assert_eq!(
        role_binding_safe(&[a, b].into(), &[a].into()),
        Err(Error::Invalid)
    );
    assert_eq!(role_binding_safe(&[a].into(), &[a, b].into()), Ok(()));
    assert_eq!(
        role_binding_safe(&BTreeSet::new(), &BTreeSet::new()),
        Ok(())
    );
}
#[test]
fn catalog_growth_limits_allow_removal_and_noops_but_reject_retired_grants() {
    let application = ApplicationId::from_u128(1).unwrap();
    let capability = CapabilityId::from_u128(2).unwrap();
    let role = RoleId::from_u128(3).unwrap();
    let resource = ResourceId::from_u128(4).unwrap();
    let scope = ScopeId::from_u128(5).unwrap();
    for (retired, grant, valid) in [
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        assert_eq!(live_capability(retired, grant).is_ok(), valid);
    }
    assert_eq!(complete_bindings(false), Ok(()));
    assert_eq!(complete_bindings(true), Err(Error::Invalid));
    for change in [
        Change::CapabilityBinding {
            application,
            capability,
            bound: true,
        },
        Change::RoleBinding {
            application,
            role,
            bound: true,
        },
        Change::RoleCapability {
            role,
            capability,
            granted: true,
        },
        Change::ResourceCapability {
            application,
            resource,
            capability,
            exposed: true,
        },
        Change::ScopeCapability {
            application,
            resource,
            scope,
            capability,
            included: true,
        },
    ] {
        assert_eq!(capacity(&change, 1000, 256, false), Err(Error::Invalid));
        assert_eq!(capacity(&change, 1000, 256, true), Ok(()));
        assert_eq!(capacity(&change, 999, 255, false), Ok(()));
        assert!(!change.needs_identifier());
    }
    assert_eq!(
        capacity(
            &Change::CapabilityBinding {
                application,
                capability,
                bound: false
            },
            1000,
            256,
            false
        ),
        Ok(())
    );
    assert_eq!(
        capacity(&Change::RetireCapability(capability), 1000, 256, false),
        Ok(())
    );
    assert!(
        Change::CreateCapability {
            definition: PermissionDefinition::new("read", "Read records").unwrap(),
            application: None
        }
        .needs_identifier()
    );
}
