use super::*;
pub(super) async fn read(tx: &mut Tx<'_>, target: ReadTarget) -> Result<Record, Error> {
    match target {
        ReadTarget::Application(id) => application(tx, id).await.map(Record::Application),
        ReadTarget::Client {
            application,
            client: id,
        } => client(tx, application, id).await.map(Record::Client),
    }
}
pub(super) async fn application(
    tx: &mut Tx<'_>,
    id: ApplicationId,
) -> Result<ApplicationRecord, Error> {
    let row = sqlx::query(
        "SELECT a.name,a.owner_id,p.email,a.active,a.revision FROM applications a JOIN principals p ON p.id=a.owner_id WHERE a.id=$1",
    )
    .bind(uuid(id.as_u128()))
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?
    .ok_or(Error::NotFound)?;
    Ok(ApplicationRecord {
        id,
        name: row.try_get("name").map_err(storage)?,
        owner: PrincipalId::from_u128(identifier(&row, "owner_id")?).map_err(storage)?,
        owner_email: row.try_get("email").map_err(storage)?,
        active: row.try_get("active").map_err(storage)?,
        revision: number(&row, "revision")?,
    })
}
pub(super) async fn configuration(tx: &mut Tx<'_>, target: ReadTarget) -> Result<Record, Error> {
    match target {
        ReadTarget::Application(id) => application(tx, id).await.map(Record::Application),
        ReadTarget::Client {
            application,
            client,
        } => client_configuration(tx, application, client)
            .await
            .map(Record::Client),
    }
}
pub(super) async fn client(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    id: ClientId,
) -> Result<ClientRecord, Error> {
    let mut record = client_configuration(tx, application, id).await?;
    let secrets=sqlx::query("SELECT id,created_ms,expires_ms FROM oauth_client_secrets WHERE client_id=$1 AND NOT retired AND (expires_ms IS NULL OR expires_ms>floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY created_ms,id").bind(uuid(id.as_u128())).fetch_all(&mut **tx).await.map_err(storage)?;
    record.secrets = secrets.iter().map(secret).collect::<Result<_, _>>()?;
    Ok(record)
}
async fn client_configuration(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    id: ClientId,
) -> Result<ClientRecord, Error> {
    let row = sqlx::query("SELECT name,active,revision,authentication_method,refresh_tokens FROM oauth_clients WHERE id=$1 AND application_id=$2")
        .bind(uuid(id.as_u128()))
        .bind(uuid(application.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    let redirects: Vec<String> = sqlx::query_scalar(
        "SELECT uri FROM client_redirects WHERE client_id=$1 ORDER BY uri LIMIT 9",
    )
    .bind(uuid(id.as_u128()))
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    let resources: Vec<Uuid> = sqlx::query_scalar(
        "SELECT resource_id FROM client_resources WHERE client_id=$1 ORDER BY resource_id LIMIT 33",
    )
    .bind(uuid(id.as_u128()))
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    let scopes: Vec<Uuid> = sqlx::query_scalar(
        "SELECT scope_id FROM client_scopes WHERE client_id=$1 ORDER BY scope_id LIMIT 129",
    )
    .bind(uuid(id.as_u128()))
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    assemble_client(&row, application, id, redirects, resources, scopes)
}
fn assemble_client(
    row: &PgRow,
    application: ApplicationId,
    id: ClientId,
    redirects: Vec<String>,
    resources: Vec<Uuid>,
    scopes: Vec<Uuid>,
) -> Result<ClientRecord, Error> {
    let mut spec = ClientSpec::new(
        Label::new(&row.try_get::<String, _>("name").map_err(storage)?)?,
        row.try_get("active").map_err(storage)?,
        crate::registration::redirects(redirects)?,
        resources
            .into_iter()
            .map(|id| ResourceId::from_u128(id.as_u128()).map_err(storage))
            .collect::<Result<_, _>>()?,
        scopes
            .into_iter()
            .map(|id| ScopeId::from_u128(id.as_u128()).map_err(storage))
            .collect::<Result<_, _>>()?,
        &row.try_get::<String, _>("authentication_method")
            .map_err(storage)?,
    )?;
    spec.refresh_tokens = row.try_get("refresh_tokens").map_err(storage)?;
    Ok(ClientRecord {
        id,
        application,
        revision: number(row, "revision")?,
        spec,
        secrets: vec![],
    })
}
fn secret(row: &PgRow) -> Result<SecretMetadata, Error> {
    Ok(SecretMetadata {
        id: ClientSecretId::from_u128(identifier(row, "id")?).map_err(storage)?,
        created_ms: number(row, "created_ms")?,
        expires_ms: optional_number(row, "expires_ms")?,
    })
}
