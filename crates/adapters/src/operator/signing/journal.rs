use crate::operator::output::Failure;
use darkhorse_application::signing_operations::{Attempt, Error, Intent, Kind};
use darkhorse_domain::{identity::OperationId, signing::Phase};
fn identifier(id: OperationId) -> String {
    uuid::Uuid::from_u128(id.as_u128()).to_string()
}
fn operation(kind: Kind) -> &'static str {
    match kind {
        Kind::Generate => "signing.generate",
        Kind::Import => "signing.import",
        Kind::Activate => "signing.activate",
        Kind::Retire => "signing.retire",
    }
}
fn phase(value: Phase) -> &'static str {
    match value {
        Phase::Staged => "staged",
        Phase::Active => "active",
        Phase::Retiring => "retiring",
        Phase::Retired => "retired",
    }
}
pub(super) fn failure(error: Error, id: OperationId) -> Failure {
    let message = match error {
        Error::Uncertain => {
            "Signing outcome is uncertain. Inspect the operation ID before any further change; do not repeat the command."
        }
        Error::Rejected(error) => super::message(error),
    };
    Failure::from(message).with_data(serde_json::json!({"operation_id":identifier(id)}))
}
pub(super) fn completed(intent: &Intent, revision: u64) -> serde_json::Value {
    let state = match intent.kind {
        Kind::Generate | Kind::Import => Phase::Staged,
        Kind::Activate => Phase::Active,
        Kind::Retire => Phase::Retired,
    };
    serde_json::json!({"operation_id":identifier(intent.id),"operation":operation(intent.kind),"kid":intent.kid,"revision":revision,"phase":phase(state),"recorded_outcome":"completed"})
}
pub(super) fn project(record: Attempt) -> serde_json::Value {
    serde_json::json!({"operation_id":identifier(record.intent.id),"operation":operation(record.intent.kind),"issuer":record.intent.issuer,"kid":record.intent.kid,"expected_revision":record.intent.expected_revision,"database_role":record.database_role,"prepared_ms":record.prepared_ms,"recorded_outcome":if record.completion.is_some() {"completed"} else {"pending"},"completed_revision":record.completion.as_ref().map(|c|c.revision),"completed_ms":record.completion.as_ref().map(|c|c.completed_ms),"current_revision":record.current_revision,"current_phase":record.current_phase.map(phase),"database_ms":record.database_ms})
}
#[cfg(test)]
#[path = "../../../tests/unit/operator/signing/journal.rs"]
mod tests;
