use super::*;
use darkhorse_domain::authentication::SessionFacts;

pub(in crate::postgres) async fn lock(tx: &mut Tx<'_>) -> Result<(), Error> {
    sqlx::query("SELECT singleton FROM security_state WHERE singleton AND NOT pg_is_in_recovery() FOR UPDATE").fetch_one(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(in crate::postgres) async fn actor(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    mutation: bool,
) -> Result<(PrincipalId, u64), Error> {
    let row = sqlx::query("SELECT s.*,p.active,p.credential_epoch AS current_epoch,NOT c.revoked AS credential_live,EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id) AS administrator FROM browser_sessions s JOIN principals p ON p.id=s.principal_id JOIN credentials c ON c.id=s.credential_id AND c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE s.digest=$1 FOR SHARE OF s,p,c,pc")
        .bind(digest.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::Unauthorized)?;
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
    checked_actor(&row, now.try_into().map_err(storage)?, mutation)
}
fn checked_actor(row: &PgRow, now: u64, mutation: bool) -> Result<(PrincipalId, u64), Error> {
    let facts = SessionFacts {
        active: row.try_get("active").map_err(storage)?,
        credential_live: row.try_get("credential_live").map_err(storage)?,
        revoked: row.try_get("revoked").map_err(storage)?,
        issued_epoch: number(row, "credential_epoch")?,
        current_epoch: number(row, "current_epoch")?,
        created_ms: number(row, "created_ms")?,
        seen_ms: number(row, "seen_ms")?,
        expires_ms: number(row, "expires_ms")?,
    };
    administrator(
        facts,
        row.try_get("administrator").map_err(storage)?,
        now,
        mutation,
    )?;
    Ok((
        PrincipalId::from_u128(identifier(row, "principal_id")?).map_err(storage)?,
        now,
    ))
}
pub(super) async fn command(tx: &mut Tx<'_>, command: &Command, now: u64) -> Result<(), Error> {
    match command {
        Command::CreateApplication(spec) => owner(tx, spec.owner).await,
        Command::UpdateApplication {
            application,
            revision,
            spec,
        } => {
            let record = records::application(tx, *application).await?;
            next_revision(record.revision, *revision)?;
            owner(tx, spec.owner).await
        }
        Command::CreateResource { application, .. } => application_exists(tx, *application).await,
        Command::CreateScope {
            application,
            resource,
            ..
        } => resource_exists(tx, *application, *resource).await,
        Command::CreateClient { application, spec } => {
            application_exists(tx, *application).await?;
            grants(tx, *application, spec).await
        }
        Command::UpdateClient {
            application,
            client,
            revision,
            spec,
        } => {
            client_revision(tx, *application, *client, *revision).await?;
            grants(tx, *application, spec).await
        }
        Command::RotateSecret {
            application,
            client,
            revision,
            overlap_seconds,
        } => {
            overlap_deadline(now, *overlap_seconds)?;
            client_revision(tx, *application, *client, *revision).await
        }
        Command::RetireSecret {
            application,
            client,
            secret,
            revision,
        } => {
            client_revision(tx, *application, *client, *revision).await?;
            let exists: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_client_secrets WHERE id=$1 AND client_id=$2 AND NOT retired)")
                .bind(uuid(secret.as_u128())).bind(uuid(client.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
            if !exists {
                return Err(Error::NotFound);
            }
            Ok(())
        }
    }
}
async fn owner(tx: &mut Tx<'_>, id: PrincipalId) -> Result<(), Error> {
    sqlx::query("SELECT id FROM principals WHERE id=$1 AND active FOR SHARE")
        .bind(uuid(id.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::Invalid)?;
    Ok(())
}
async fn application_exists(tx: &mut Tx<'_>, id: ApplicationId) -> Result<(), Error> {
    sqlx::query("SELECT id FROM applications WHERE id=$1")
        .bind(uuid(id.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    Ok(())
}
async fn resource_exists(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    resource: ResourceId,
) -> Result<(), Error> {
    sqlx::query("SELECT id FROM protected_resources WHERE application_id=$1 AND id=$2")
        .bind(uuid(application.as_u128()))
        .bind(uuid(resource.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    Ok(())
}
async fn client_revision(
    tx: &mut Tx<'_>,
    app: ApplicationId,
    client: ClientId,
    revision: u64,
) -> Result<(), Error> {
    let current: i64 =
        sqlx::query_scalar("SELECT revision FROM oauth_clients WHERE application_id=$1 AND id=$2")
            .bind(uuid(app.as_u128()))
            .bind(uuid(client.as_u128()))
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
    next_revision(current.try_into().map_err(storage)?, revision)?;
    Ok(())
}
async fn grants(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    spec: &ClientSpec,
) -> Result<(), Error> {
    validate_spec(spec)?;
    let rows = sqlx::query("SELECT id,application_id FROM protected_resources WHERE id=ANY($1)")
        .bind(
            spec.resources
                .iter()
                .map(|id| uuid(id.as_u128()))
                .collect::<Vec<_>>(),
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    let resources = rows
        .iter()
        .map(resource_pair)
        .collect::<Result<Vec<_>, _>>()?;
    let rows =
        sqlx::query("SELECT id,application_id,resource_id FROM resource_scopes WHERE id=ANY($1)")
            .bind(
                spec.scopes
                    .iter()
                    .map(|id| uuid(id.as_u128()))
                    .collect::<Vec<_>>(),
            )
            .fetch_all(&mut **tx)
            .await
            .map_err(storage)?;
    let scopes = rows
        .iter()
        .map(scope_tuple)
        .collect::<Result<Vec<_>, _>>()?;
    grants_match(application, spec, &resources, &scopes)
}
fn validate_spec(spec: &ClientSpec) -> Result<(), Error> {
    ClientSpec::new(
        spec.name.clone(),
        spec.active,
        crate::registration::redirects(spec.redirects.values().to_vec())?,
        spec.resources.clone(),
        spec.scopes.clone(),
        "client_secret_basic",
    )?;
    Ok(())
}
fn resource_pair(row: &PgRow) -> Result<(ResourceId, ApplicationId), Error> {
    Ok((
        ResourceId::from_u128(identifier(row, "id")?).map_err(storage)?,
        ApplicationId::from_u128(identifier(row, "application_id")?).map_err(storage)?,
    ))
}
fn scope_tuple(row: &PgRow) -> Result<(ScopeId, ResourceId, ApplicationId), Error> {
    Ok((
        ScopeId::from_u128(identifier(row, "id")?).map_err(storage)?,
        ResourceId::from_u128(identifier(row, "resource_id")?).map_err(storage)?,
        ApplicationId::from_u128(identifier(row, "application_id")?).map_err(storage)?,
    ))
}
