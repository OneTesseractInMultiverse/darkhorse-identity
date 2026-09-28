use super::*;

pub(super) async fn insert(
    tx: &mut Tx<'_>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    outcome: &Result<RotatedSecret, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let (result, target_revision, secret_id) = result(request.revision(), outcome)?;
    let target = request.target();
    let inserted = sqlx::query("INSERT INTO operator_client_secret_rotation_audit(operation_id,application_id,client_id,expected_revision,target_revision,secret_id,overlap_seconds,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
        .bind(Uuid::from_u128(id.as_u128()))
        .bind(Uuid::from_u128(target.application.as_u128()))
        .bind(Uuid::from_u128(target.client.as_u128()))
        .bind(i64::try_from(request.revision()).map_err(storage)?)
        .bind(target_revision)
        .bind(secret_id)
        .bind(i16::try_from(request.overlap_seconds()).map_err(storage)?)
        .bind(request.reason())
        .bind(actor.map(|a| Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch).map_err(storage)).transpose()?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms).map_err(storage)).transpose()?)
        .bind(result)
        .bind(i64::try_from(now).map_err(storage)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}

fn result(
    expected_revision: u64,
    outcome: &Result<RotatedSecret, Error>,
) -> Result<(&'static str, Option<i64>, Option<Uuid>), Error> {
    match outcome {
        Ok(rotated) => {
            if rotated.revision != expected_revision + 1 {
                return Err(Error::Unavailable);
            }
            Ok((
                "written",
                Some(i64::try_from(rotated.revision).map_err(storage)?),
                Some(Uuid::from_u128(rotated.secret_id.as_u128())),
            ))
        }
        Err(Error::Denied) => Ok(("denied", None, None)),
        Err(Error::Invalid) => Ok(("invalid", None, None)),
        Err(Error::NotFound) => Ok(("not_found", None, None)),
        Err(Error::Conflict) => Ok(("conflict", None, None)),
        _ => Err(Error::Unavailable),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/postgres/operator_client_rotation/audit.rs"]
mod tests;
