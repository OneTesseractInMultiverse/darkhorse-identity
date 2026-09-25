use super::*;
pub(super) async fn insert(
    tx: &mut Tx<'_>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    outcome: &Result<Outcome, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let (command, secret, revision, after, limit) = selectors(request);
    let (result, count) = result(request.operation(), outcome)?;
    let target = request.target();
    let inserted=sqlx::query("INSERT INTO operator_client_secret_audit(operation_id,command,application_id,client_id,secret_id,expected_revision,target_revision,cursor_id,page_size,returned_count,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
 .bind(Uuid::from_u128(id.as_u128())).bind(command).bind(Uuid::from_u128(target.application.as_u128())).bind(Uuid::from_u128(target.client.as_u128()))
 .bind(secret).bind(revision).bind(outcome.as_ref().ok().map(|o|i64::try_from(o.revision())).transpose().map_err(storage)?)
 .bind(after).bind(limit).bind(count).bind(request.reason())
 .bind(actor.map(|a|Uuid::from_u128(a.credential.principal.as_u128()))).bind(actor.map(|a|Uuid::from_u128(a.credential.credential.as_u128())))
 .bind(actor.map(|a|i64::try_from(a.credential.epoch)).transpose().map_err(storage)?).bind(actor.map(|a|i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
 .bind(result).bind(i64::try_from(now).map_err(storage)?).execute(&mut **tx).await.map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
type Selectors = (
    &'static str,
    Option<Uuid>,
    Option<i64>,
    Option<Uuid>,
    Option<i16>,
);
fn selectors(request: &Request) -> Selectors {
    match request.operation() {
        Operation::List { after, limit } => (
            "client.secret.list",
            None,
            None,
            after.map(|id| Uuid::from_u128(id.as_u128())),
            Some(limit as i16),
        ),
        Operation::Retire { secret, revision } => (
            "client.secret.retire",
            Some(Uuid::from_u128(secret.as_u128())),
            Some(revision as i64),
            None,
            None,
        ),
    }
}
fn result(
    operation: Operation,
    outcome: &Result<Outcome, Error>,
) -> Result<(&'static str, Option<i16>), Error> {
    match (operation, outcome) {
        (Operation::List { limit, .. }, Ok(Outcome::Listed(p)))
            if p.items.len() <= usize::from(limit) =>
        {
            Ok(("read", Some(p.items.len() as i16)))
        }
        (Operation::Retire { .. }, Ok(Outcome::Retired { .. })) => Ok(("written", None)),
        (_, Err(Error::Denied)) => Ok(("denied", None)),
        (_, Err(Error::NotFound)) => Ok(("not_found", None)),
        (Operation::Retire { .. }, Err(Error::Conflict)) => Ok(("conflict", None)),
        _ => Err(Error::Unavailable),
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/postgres/operator_client_secrets/audit.rs"]
mod tests;
