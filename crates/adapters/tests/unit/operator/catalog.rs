use super::*;
use crate::operator::output::{Format, OUTPUT_LIMIT, render};
use darkhorse_application::{
    admin_catalog::{ClientSummary, RoleSummary},
    registration::ApplicationRecord,
};
use darkhorse_domain::identity::{ApplicationId, ClientId, PrincipalId, RoleId};
fn application() -> ApplicationRecord {
    ApplicationRecord {
        id: ApplicationId::from_u128(1).unwrap(),
        name: "\u{202e}".repeat(100),
        owner: PrincipalId::from_u128(1).unwrap(),
        owner_email: "x".repeat(254),
        active: true,
        revision: i64::MAX as u64,
    }
}
fn client() -> ClientSummary {
    ClientSummary {
        id: ClientId::from_u128(1).unwrap(),
        application: application().id,
        name: "\u{202e}".repeat(100),
        active: false,
        revision: 17,
    }
}
#[test]
fn bounded_pages_escape_terminal_data_and_include_only_public_catalog_fields() {
    for (target, item, count) in [
        (Target::Applications, Item::Application(application()), 6),
        (Target::Clients(application().id), Item::Client(client()), 5),
    ] {
        let record = output(
            OperationId::from_u128(1).unwrap(),
            target,
            Page {
                items: vec![item; 25],
                next: Some(std::num::NonZeroU128::new(1).unwrap()),
                policy_revision: i64::MAX as u64,
            },
        )
        .unwrap();
        assert_eq!(record.data["items"][0].as_object().unwrap().len(), count);
        assert!(record.data["items"][0]["revision"].is_string());
        assert!(record.data["policy_revision"].is_string());
        for format in [Format::Human, Format::Json] {
            let bytes = render(&record, format).unwrap();
            assert!(bytes.len() < OUTPUT_LIMIT);
            assert!(
                !String::from_utf8(bytes.clone())
                    .unwrap()
                    .contains('\u{202e}')
            );
            let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        }
    }
}
#[test]
fn mismatched_types_and_foreign_clients_are_not_projected() {
    assert!(project(Target::Applications, Item::Client(client())).is_err());
    assert!(
        project(
            Target::Clients(ApplicationId::from_u128(2).unwrap()),
            Item::Client(client())
        )
        .is_err()
    );
    assert!(
        project(
            Target::Applications,
            Item::Role(RoleSummary {
                id: RoleId::from_u128(1).unwrap(),
                name: "role".into()
            })
        )
        .is_err()
    );
    let result = output(
        OperationId::from_u128(1).unwrap(),
        Target::Applications,
        Page {
            items: vec![],
            next: None,
            policy_revision: 0,
        },
    )
    .unwrap();
    assert_eq!(result.data["items"], serde_json::json!([]));
    assert!(result.data["next"].is_null());
    for error in [
        Error::Denied,
        Error::NotFound,
        Error::Invalid,
        Error::Unavailable,
        Error::Conflict,
        Error::PolicyRejected,
        Error::Uncertain,
        Error::Limited {
            retry_after_ms: 123,
        },
    ] {
        let rendered = crate::operator::output::render_failure(
            &failure(error, OperationId::from_u128(1).unwrap()),
            Format::Json,
        );
        let value: serde_json::Value = serde_json::from_slice(&rendered.unwrap()).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "operation_failed");
        assert_eq!(
            value["data"]["operation_id"],
            "00000000-0000-0000-0000-000000000001"
        );
        if let Error::Limited { retry_after_ms } = error {
            assert_eq!(value["data"]["retry_after_ms"], retry_after_ms);
        } else {
            assert!(value["data"].get("retry_after_ms").is_none());
        }
    }
}

#[test]
fn access_pages_use_bounded_metadata_projections_and_reject_foreign_records() {
    use darkhorse_application::{
        admin_catalog::CapabilitySummary,
        registration::{ResourceRecord, ScopeRecord},
    };
    use darkhorse_domain::{
        identity::{CapabilityId, ResourceId, ScopeId},
        operator_catalog::Definitions,
    };
    let app = application().id;
    let resource = ResourceRecord {
        id: ResourceId::from_u128(3).unwrap(),
        application: app,
        name: "\u{202e}".repeat(100),
        audience: "urn:darkhorse:resource:00000000-0000-0000-0000-000000000003".into(),
    };
    let scope = ScopeRecord {
        id: ScopeId::from_u128(4).unwrap(),
        application: app,
        resource: resource.id,
        name: "x".repeat(100),
    };
    for (target, item, fields) in [
        (Target::Resources(app), Item::Resource(resource.clone()), 4),
        (Target::Scopes(app), Item::Scope(scope.clone()), 4),
        (
            Target::Roles(Definitions::All),
            Item::Role(RoleSummary {
                id: RoleId::from_u128(5).unwrap(),
                name: "\u{202e}".repeat(100),
            }),
            2,
        ),
        (
            Target::Capabilities(Definitions::Application(app)),
            Item::Capability(CapabilitySummary {
                id: CapabilityId::from_u128(6).unwrap(),
                key: "x".repeat(200),
                meaning: "private-description-marker".repeat(1000),
                retired: true,
            }),
            3,
        ),
    ] {
        let record = output(
            OperationId::from_u128(1).unwrap(),
            target,
            Page {
                items: vec![item; 25],
                next: None,
                policy_revision: 0,
            },
        )
        .unwrap();
        assert_eq!(record.data["items"][0].as_object().unwrap().len(), fields);
        for format in [Format::Human, Format::Json] {
            let bytes = render(&record, format).unwrap();
            assert!(bytes.len() < OUTPUT_LIMIT);
            let text = String::from_utf8(bytes).unwrap();
            assert!(!text.contains('\u{202e}') && !text.contains("private-description-marker"));
        }
    }
    let foreign = ApplicationId::from_u128(9).unwrap();
    assert!(project(Target::Resources(foreign), Item::Resource(resource)).is_err());
    assert!(project(Target::Scopes(foreign), Item::Scope(scope)).is_err());
}
