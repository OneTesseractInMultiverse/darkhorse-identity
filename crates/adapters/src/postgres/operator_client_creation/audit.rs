use super::*;
use uuid::Uuid;

pub(super) async fn insert(
    tx: &mut Tx<'_>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    outcome: &Result<CreatedClient, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let target = outcome
        .as_ref()
        .ok()
        .map(|created| Uuid::from_u128(created.record.id.as_u128()));
    let inserted = sqlx::query("INSERT INTO operator_client_creation_audit(operation_id,application_id,client_id,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(Uuid::from_u128(id.as_u128()))
        .bind(Uuid::from_u128(request.application().as_u128()))
        .bind(target)
        .bind(request.reason())
        .bind(actor.map(|a| Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(result(outcome)?)
        .bind(i64::try_from(now).map_err(storage)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}

fn result(outcome: &Result<CreatedClient, Error>) -> Result<&'static str, Error> {
    match outcome {
        Ok(_) => Ok("written"),
        Err(Error::Denied) => Ok("denied"),
        Err(Error::Invalid) => Ok("invalid"),
        Err(Error::NotFound) => Ok("not_found"),
        Err(Error::Conflict) => Ok("conflict"),
        _ => Err(Error::Unavailable),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/postgres/operator_client_creation/audit.rs"]
mod tests;
