use super::*;
use darkhorse_domain::{AccountStatus, directory::AccountAction};
pub(super) async fn insert(
    tx: &mut Tx<'_>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    outcome: &Result<Outcome, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let values = values(request.operation());
    let inserted = sqlx::query("INSERT INTO operator_account_audit(operation_id,command,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,target_id,expected_revision,target_revision,reason,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(Uuid::from_u128(id.as_u128())).bind(values.0)
        .bind(actor.map(|a|Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a|Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a|i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a|i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(values.1).bind(values.2)
        .bind(outcome.as_ref().ok().map(|r|r.account.revision as i64))
        .bind(request.reason()).bind(result(request.operation(),outcome)?).bind(now as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn values(operation: Operation) -> (&'static str, Uuid, Option<i64>) {
    match operation {
        Operation::Show(target) => ("account.show", Uuid::from_u128(target.as_u128()), None),
        Operation::Change {
            target,
            revision,
            action,
        } => (
            match action {
                AccountAction::SetStatus(AccountStatus::Inactive) => "account.deactivate",
                AccountAction::SetStatus(AccountStatus::Active) => "account.reactivate",
                AccountAction::RevokeAll => "account.revoke_all",
            },
            Uuid::from_u128(target.as_u128()),
            Some(revision as i64),
        ),
    }
}
fn result(operation: Operation, outcome: &Result<Outcome, Error>) -> Result<&'static str, Error> {
    match outcome {
        Ok(_) if matches!(operation, Operation::Show(_)) => Ok("read"),
        Ok(r) => Ok(if r.changed { "changed" } else { "unchanged" }),
        Err(Error::Denied) => Ok("denied"),
        Err(Error::NotFound) => Ok("not_found"),
        Err(Error::Conflict) => Ok("conflict"),
        Err(Error::PolicyRejected) => Ok("policy_rejected"),
        Err(_) => Err(Error::Unavailable),
    }
}
