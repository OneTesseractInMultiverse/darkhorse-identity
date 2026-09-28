use super::localization::Locale;
use super::{
    authenticated,
    catalog::failure,
    output::{Failure, Output},
};
use crate::password::PasswordPreparation;
use darkhorse_application::{
    admin_catalog::{CapabilitySummary, Item, View},
    operator_accounts,
};
use darkhorse_domain::{
    identity::OperationId, operator_accounts::Error, operator_catalog::ViewRequest,
};

const RELATED_RECORD_LIMIT: usize = 25;

pub(super) async fn run(
    request: ViewRequest,
    stdin: bool,
    locale: Locale,
) -> Result<Output, Failure> {
    let input = authenticated::credentials(stdin, false, locale).await?;
    if input.reason.is_some() {
        return Err(Failure::usage());
    }
    let operation = super::operation_id()?;
    perform(operation, request, &input)
        .await
        .map_err(|error| failure(error, operation))
}

async fn perform(
    operation: OperationId,
    request: ViewRequest,
    input: &authenticated::Input,
) -> Result<Output, Error> {
    let context = authenticated::connect().await?;
    let view = operator_accounts::run(
        &context.store.operator_catalog_views(),
        &context.admission,
        &PasswordPreparation::default(),
        operation,
        request,
        &input.email,
        &input.password,
    )
    .await;
    context.store.close().await;
    output(operation, request, view?)
}

fn output(operation: OperationId, request: ViewRequest, view: View) -> Result<Output, Error> {
    let target = request.target();
    if view.applications.len() > RELATED_RECORD_LIMIT
        || view.capabilities.len() > RELATED_RECORD_LIMIT
    {
        return Err(Error::Unavailable);
    }
    let record = project(target, view)?;
    Ok(Output::record(serde_json::json!({
        "operation_id": id(operation.as_u128()),
        "record": record
    })))
}

fn project(
    target: darkhorse_domain::operator_catalog::ViewTarget,
    view: View,
) -> Result<serde_json::Value, Error> {
    use darkhorse_application::registration::{ResourceRecord, ScopeRecord};
    use darkhorse_domain::operator_catalog::{Definitions, ViewTarget};

    let item = match (target, view.item) {
        (ViewTarget::Capability { id: expected, .. }, Item::Capability(value))
            if value.id == expected =>
        {
            serde_json::json!({
                "kind":"capability",
                "id":id(value.id.as_u128()),
                "key":value.key,
                "meaning":value.meaning,
                "retired":value.retired
            })
        }
        (ViewTarget::Role { id: expected, .. }, Item::Role(value)) if value.id == expected => {
            serde_json::json!({
                "kind":"role",
                "id":id(value.id.as_u128()),
                "name":value.name
            })
        }
        (
            ViewTarget::Resource {
                application,
                id: expected,
            },
            Item::Resource(ResourceRecord {
                id,
                application: owner,
                name,
                audience,
            }),
        ) if id == expected && owner == application => serde_json::json!({
            "kind":"resource",
            "id":id_string(id.as_u128()),
            "application_id":id_string(owner.as_u128()),
            "name":name,
            "audience":audience
        }),
        (
            ViewTarget::Scope {
                application,
                resource,
                id: expected,
            },
            Item::Scope(ScopeRecord {
                id,
                application: owner,
                resource: parent,
                name,
            }),
        ) if id == expected && owner == application && parent == resource => serde_json::json!({
            "kind":"scope",
            "id":id_string(id.as_u128()),
            "application_id":id_string(owner.as_u128()),
            "resource_id":id_string(parent.as_u128()),
            "name":name
        }),
        _ => return Err(Error::Unavailable),
    };

    let applications = view
        .applications
        .into_iter()
        .map(|application| {
            serde_json::json!({
                "id":id_string(application.id.as_u128()),
                "name":application.name,
                "active":application.active
            })
        })
        .collect::<Vec<_>>();
    let capabilities = view
        .capabilities
        .into_iter()
        .map(capability_reference)
        .collect::<Vec<_>>();
    let scope = match target {
        ViewTarget::Capability {
            selection: Definitions::All,
            ..
        }
        | ViewTarget::Role {
            selection: Definitions::All,
            ..
        } => "all-definitions",
        ViewTarget::Capability {
            selection: Definitions::Application(_),
            ..
        }
        | ViewTarget::Role {
            selection: Definitions::Application(_),
            ..
        } => "application",
        _ => "application",
    };
    Ok(serde_json::json!({
        "item":item,
        "definition_scope":scope,
        "applications":applications,
        "capabilities":capabilities,
        "policy_revision":view.policy_revision.to_string()
    }))
}

fn capability_reference(value: CapabilitySummary) -> serde_json::Value {
    serde_json::json!({
        "id":id_string(value.id.as_u128()),
        "key":value.key,
        "retired":value.retired
    })
}

fn id(value: u128) -> String {
    id_string(value)
}

fn id_string(value: u128) -> String {
    uuid::Uuid::from_u128(value).to_string()
}

#[cfg(test)]
#[path = "../../tests/unit/operator/catalog_views.rs"]
mod tests;
