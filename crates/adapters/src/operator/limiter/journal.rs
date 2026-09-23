use crate::operator::output::Failure;
use darkhorse_application::limiter_activation::{Attempt, Error};
use darkhorse_domain::identity::OperationId;
pub(super) fn operation_id() -> Result<OperationId, Failure> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Cannot generate operation correlation.")?;
    OperationId::from_u128(
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .as_u128(),
    )
    .map_err(|_| "Cannot generate operation correlation.".into())
}
pub(super) fn failure(error: Error, id: OperationId) -> Failure {
    let message = match error {
        Error::NotReady => {
            "Limiter recovery wait has not elapsed, or the generation is already active."
        }
        Error::NotFound => "Limiter activation record not found.",
        Error::Unavailable => {
            "Limiter activation journal unavailable; check operator configuration, schema and grants. Inspect state before any retry."
        }
        Error::Uncertain => {
            "Limiter activation outcome unknown; inspect the operation record and enforcement state before any retry."
        }
    };
    Failure::from(message).with_data(
        serde_json::json!({"operation_id":uuid::Uuid::from_u128(id.as_u128()).to_string()}),
    )
}
pub(super) fn project(attempt: Attempt) -> serde_json::Value {
    serde_json::json!({
    "operation_id":uuid::Uuid::from_u128(attempt.id.as_u128()).to_string(),
    "operation":"limiter.activate", "recorded_outcome":if attempt.completion.is_some(){"activated"}else{"pending"},
    "database_role":attempt.database_role,"epoch":attempt.generation.epoch(),
    "prepared_ms":attempt.prepared_ms,"not_before_ms":attempt.not_before_ms,
    "completed_ms":attempt.completion.map(|v|v.completed_ms),
    "current_epoch":attempt.current.generation.epoch(),"current_phase":if attempt.current.active{"active"}else{"cooling"},
    "same_generation":attempt.current.generation==attempt.generation,
    "database_ms":attempt.current.now_ms
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/operator/limiter/journal.rs"]
mod tests;
