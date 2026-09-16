use super::*;
use crate::authorization::fixtures::*;
use std::collections::BTreeSet;

#[test]
fn accepts_consistent_catalog_and_empty_unbound_definitions() {
    assert!(Catalog::new(definitions()).is_ok());
    assert!(Catalog::new(Definitions::default()).is_ok());
    let mut defs = definitions();
    defs.roles[0].applications.clear();
    assert!(Catalog::new(defs).is_ok());
}

#[test]
fn rejects_duplicate_identifiers_in_every_catalog() {
    macro_rules! duplicate {
        ($field:ident, $kind:ident) => {{
            let mut defs = definitions();
            defs.$field.push(defs.$field[0].clone());
            assert!(matches!(
                Catalog::new(defs),
                Err(CatalogError::Duplicate(DefinitionKind::$kind))
            ));
        }};
    }
    duplicate!(applications, Application);
    duplicate!(resources, Resource);
    duplicate!(capabilities, Capability);
    duplicate!(roles, Role);
    duplicate!(scopes, Scope);
    duplicate!(clients, Client);
}

#[test]
fn rejects_missing_references_and_cross_application_capabilities() {
    type Change = fn(&mut Definitions);
    let cases: [(Change, CatalogError); 13] = [
        (
            |d| d.capabilities[0].applications = BTreeSet::from([app(99)]),
            CatalogError::UnknownApplication,
        ),
        (
            |d| d.resources[0].application = app(99),
            CatalogError::UnknownApplication,
        ),
        (
            |d| d.roles[0].applications = BTreeSet::from([app(99)]),
            CatalogError::UnknownApplication,
        ),
        (
            |d| d.clients[0].application = app(99),
            CatalogError::UnknownApplication,
        ),
        (
            |d| d.resources[0].capabilities = caps(&[99]),
            CatalogError::UnknownCapability,
        ),
        (
            |d| d.roles[0].capabilities = caps(&[99]),
            CatalogError::UnknownCapability,
        ),
        (
            |d| d.scopes[0].capabilities = caps(&[99]),
            CatalogError::UnknownCapability,
        ),
        (
            |d| d.resources[0].capabilities = caps(&[4]),
            CatalogError::CapabilityBindingMismatch,
        ),
        (
            |d| d.roles[0].capabilities = caps(&[4]),
            CatalogError::CapabilityBindingMismatch,
        ),
        (
            |d| d.scopes[0].resource = resource(99),
            CatalogError::UnknownResource,
        ),
        (
            |d| {
                d.clients[0].resources.insert(resource(99));
            },
            CatalogError::UnknownResource,
        ),
        (
            |d| {
                d.clients[0].scopes.insert(scope(99));
            },
            CatalogError::UnknownScope,
        ),
        (
            |d| {
                d.clients[0].scopes.insert(scope(3));
            },
            CatalogError::ScopeResourceMismatch,
        ),
    ];
    for (change, expected) in cases {
        let mut defs = definitions();
        change(&mut defs);
        assert_eq!(Catalog::new(defs).unwrap_err(), expected);
    }
}

#[test]
fn scope_cannot_expose_capabilities_its_resource_does_not_expose() {
    let mut defs = definitions();
    defs.scopes[0].capabilities = caps(&[4]);
    assert_eq!(
        Catalog::new(defs).unwrap_err(),
        CatalogError::CapabilityBindingMismatch
    );
}
