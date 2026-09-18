use super::*;
pub(super) async fn apply(
    tx: &mut Tx<'_>,
    command: Command,
    secret: Option<Verifier>,
    now: u64,
) -> Result<(), Error> {
    match command.change {
        Change::Register => {
            sqlx::query(
                "INSERT INTO resource_introspection(resource_id,application_id) VALUES($1,$2)",
            )
            .bind(uuid(command.target.resource.as_u128()))
            .bind(uuid(command.target.application.as_u128()))
            .execute(&mut **tx)
            .await
            .map_err(storage)?;
            insert_secret(
                tx,
                command.target.resource,
                secret.ok_or(Error::Invalid)?,
                now,
            )
            .await
        }
        Change::Rotate {
            overlap_seconds, ..
        } => {
            rotate(
                tx,
                command.target.resource,
                overlap_deadline(now, overlap_seconds)?,
            )
            .await?;
            insert_secret(
                tx,
                command.target.resource,
                secret.ok_or(Error::Invalid)?,
                now,
            )
            .await?;
            advance(tx, command.target.resource).await
        }
        Change::SetActive { active, .. } => {
            no_secret(secret)?;
            set_active(tx, command.target.resource, active, now).await
        }
    }
}
fn no_secret(secret: Option<Verifier>) -> Result<(), Error> {
    if secret.is_some() {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
async fn insert_secret(
    tx: &mut Tx<'_>,
    resource: ResourceId,
    secret: Verifier,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO resource_introspection_secrets(id,resource_id,verifier,created_ms) VALUES($1,$2,$3,$4)")
        .bind(uuid(secret.id.as_u128())).bind(uuid(resource.as_u128())).bind(secret.digest.as_slice()).bind(now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn rotate(tx: &mut Tx<'_>, resource: ResourceId, deadline: u64) -> Result<(), Error> {
    sqlx::query("UPDATE resource_introspection_secrets SET retired=true WHERE resource_id=$1 AND expires_ms IS NOT NULL AND NOT retired")
        .bind(uuid(resource.as_u128())).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("UPDATE resource_introspection_secrets SET expires_ms=$2 WHERE resource_id=$1 AND expires_ms IS NULL AND NOT retired")
        .bind(uuid(resource.as_u128())).bind(deadline as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn advance(tx: &mut Tx<'_>, resource: ResourceId) -> Result<(), Error> {
    sqlx::query("UPDATE resource_introspection SET revision=revision+1 WHERE resource_id=$1")
        .bind(uuid(resource.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
async fn set_active(
    tx: &mut Tx<'_>,
    resource: ResourceId,
    active: bool,
    now: u64,
) -> Result<(), Error> {
    if active {
        sqlx::query("SELECT id FROM resource_introspection_secrets WHERE resource_id=$1 AND NOT retired AND created_ms<=$2 AND expires_ms IS NULL")
            .bind(uuid(resource.as_u128())).bind(now as i64).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::Invalid)?;
    } else {
        sqlx::query("UPDATE resource_introspection_secrets SET retired=true WHERE resource_id=$1 AND NOT retired")
            .bind(uuid(resource.as_u128())).execute(&mut **tx).await.map_err(storage)?;
    }
    sqlx::query(
        "UPDATE resource_introspection SET active=$2,revision=revision+1 WHERE resource_id=$1",
    )
    .bind(uuid(resource.as_u128()))
    .bind(active)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}
pub(super) async fn audit(
    tx: &mut Tx<'_>,
    actor: PrincipalId,
    command: Command,
    record: &Record,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO resource_registration_audit(actor_id,resource_id,revision,event,occurred_ms) VALUES($1,$2,$3,$4,$5)")
        .bind(uuid(actor.as_u128())).bind(uuid(record.target.resource.as_u128())).bind(record.revision as i64).bind(event(command.change)).bind(now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
fn event(change: Change) -> &'static str {
    match change {
        Change::Register => "registered",
        Change::Rotate { .. } => "rotated",
        Change::SetActive { active: true, .. } => "enabled",
        Change::SetActive { active: false, .. } => "disabled",
    }
}
