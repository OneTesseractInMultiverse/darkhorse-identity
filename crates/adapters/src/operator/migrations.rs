use super::output::{Failure, Output};
use crate::postgres::{
    PostgresStore,
    migrations::{Error, Inspection},
};
use darkhorse_domain::identity::OperationId;
fn identifier(id: OperationId) -> String {
    uuid::Uuid::from_u128(id.as_u128()).to_string()
}
pub(super) async fn apply(store: &PostgresStore) -> Result<Output, Failure> {
    let id = super::operation_id()?;
    store
        .migrate_operation(id)
        .await
        .map_err(|error| failure(error, id))?;
    Ok(Output::message(
        format!("Database migrations applied. Operation: {}", identifier(id)),
        serde_json::json!({"migrated":true,"operation_id":identifier(id),"recorded_outcome":"completed"}),
    ))
}
pub(super) async fn inspect(store: &PostgresStore, id: OperationId) -> Result<Output, Failure> {
    let record = store
        .inspect_migration(id)
        .await
        .map_err(|error| failure(error, id))?;
    Ok(Output::record(match record {
        Some(record) => project(id, record),
        None => serde_json::json!({"operation_id":identifier(id),"recorded_outcome":"absent"}),
    }))
}
fn failure(error: Error, id: OperationId) -> Failure {
    let message = match error {
        Error::Incompatible => {
            "Migration history or embedded manifest is incompatible. Stop and review the database with the matching release."
        }
        Error::Unavailable => {
            "Migration storage or database-owner authority is unavailable. Inspect the operation and database before any further change."
        }
        Error::Uncertain => {
            "Migration outcome is uncertain. Inspect the operation ID before any further change; do not repeat the command."
        }
    };
    Failure::from(message).with_data(serde_json::json!({"operation_id":identifier(id)}))
}
fn project(id: OperationId, record: Inspection) -> serde_json::Value {
    let steps = record.steps.into_iter().map(|s|serde_json::json!({"version":s.version,"checksum":s.checksum,"already_applied":s.already_applied,"completed_ms":s.completed_ms,"current_matches":s.current_matches})).collect::<Vec<_>>();
    serde_json::json!({"operation_id":identifier(id),"operation":"migrate","recorded_outcome":if record.completed_ms.is_some() {"completed"} else {"pending"},"database_role":record.database_role,"prepared_ms":record.prepared_ms,"completed_ms":record.completed_ms,"database_ms":record.database_ms,"steps":steps})
}
#[cfg(test)]
#[path = "../../tests/unit/operator/migrations.rs"]
mod tests;
