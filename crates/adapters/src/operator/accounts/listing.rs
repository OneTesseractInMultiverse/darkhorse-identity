use crate::operator::output::Output;
use darkhorse_application::admin_directory::Page;
use darkhorse_domain::{AccountStatus, identity::OperationId};

pub(super) fn output(id: OperationId, page: Page) -> Output {
    let users: Vec<_> = page
        .items
        .into_iter()
        .map(|u| {
            serde_json::json!({
                "id": uuid::Uuid::from_u128(u.id.as_u128()).to_string(),
                "email":u.email,"first_name":u.first_name,"last_name":u.last_name,
                "active":u.status==AccountStatus::Active,"administrator":u.administrator,
                "email_verified":u.email_verified,"revision":u.revision.to_string()
            })
        })
        .collect();
    Output::record(serde_json::json!({
        "operation_id": uuid::Uuid::from_u128(id.as_u128()).to_string(),
        "items": users,
        "next": page.next.map(|id| uuid::Uuid::from_u128(id.as_u128()).to_string())
    }))
}

#[cfg(test)]
#[path = "../../../tests/unit/operator/accounts/listing.rs"]
mod tests;
