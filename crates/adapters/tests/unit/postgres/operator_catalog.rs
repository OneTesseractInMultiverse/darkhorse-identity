use super::*;
use darkhorse_domain::{identity::ApplicationId, operator_catalog::Definitions};
#[test]
fn target_mapping_preserves_explicit_scope_and_command() {
    let app = ApplicationId::from_u128(7).unwrap();
    for (target, command, application) in [
        (Target::Applications, "application.list", None),
        (Target::Clients(app), "client.list", Some(app)),
        (Target::Resources(app), "resource.list", Some(app)),
        (Target::Scopes(app), "scope.list", Some(app)),
        (
            Target::Roles(Definitions::Application(app)),
            "role.list",
            Some(app),
        ),
        (
            Target::Capabilities(Definitions::Application(app)),
            "capability.list",
            Some(app),
        ),
        (Target::Roles(Definitions::All), "role.list", None),
        (
            Target::Capabilities(Definitions::All),
            "capability.list",
            None,
        ),
    ] {
        assert_eq!(
            audit_target(target),
            (command, application.map(|id| Uuid::from_u128(id.as_u128())))
        );
        use darkhorse_application::admin_catalog::List;
        let mapped = selection(target);
        match (target, mapped) {
            (Target::Applications, List::Applications) => {}
            (Target::Clients(a), List::Clients(b))
            | (Target::Resources(a), List::Resources(b))
            | (Target::Scopes(a), List::Scopes(b)) => assert_eq!(a, b),
            (Target::Roles(a), List::Roles(b))
            | (Target::Capabilities(a), List::Capabilities(b)) => assert_eq!(a.application(), b),
            _ => panic!("wrong catalog"),
        }
    }
}
