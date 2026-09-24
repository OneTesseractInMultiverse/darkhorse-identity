use super::*;
pub(super) async fn apply(
    tx: &mut Tx<'_>,
    command: &Command,
    prepared: Prepared,
    now: u64,
) -> Result<Record, Error> {
    match command {
        Command::CreateApplication(spec) => create_application(tx, spec, new_id(&prepared)?).await,
        Command::UpdateApplication {
            application,
            revision,
            spec,
        } => update_application(tx, *application, *revision, spec).await,
        Command::CreateResource { application, name } => {
            create_resource(tx, *application, name, new_id(&prepared)?).await
        }
        Command::CreateScope {
            application,
            resource,
            name,
        } => create_scope(tx, *application, *resource, name, new_id(&prepared)?).await,
        Command::CreateClient { application, spec } => {
            create_client(tx, *application, spec, prepared, now).await
        }
        Command::UpdateClient {
            application,
            client,
            revision,
            spec,
        } => update_client(tx, *application, *client, *revision, spec).await,
        Command::RotateSecret {
            application,
            client,
            revision,
            overlap_seconds,
        } => {
            rotate(
                tx,
                *client,
                prepared.secret.ok_or(Error::Unavailable)?,
                now,
                *overlap_seconds,
            )
            .await?;
            updated_client(tx, *application, *client, *revision).await
        }
        Command::RetireSecret {
            application,
            client,
            secret,
            revision,
        } => {
            retire(tx, *client, *secret).await?;
            updated_client(tx, *application, *client, *revision).await
        }
    }
}
async fn create_application(
    tx: &mut Tx<'_>,
    spec: &ApplicationSpec,
    raw: u128,
) -> Result<Record, Error> {
    let id = ApplicationId::from_u128(raw).map_err(storage)?;
    let inserted =
        sqlx::query("INSERT INTO applications (id,name,owner_id,active) VALUES ($1,$2,$3,$4)")
            .bind(uuid(raw))
            .bind(spec.name.as_str())
            .bind(uuid(spec.owner.as_u128()))
            .bind(spec.active)
            .execute(&mut **tx)
            .await
            .map_err(constraint)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    records::application(tx, id).await.map(Record::Application)
}
async fn update_application(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    revision: u64,
    spec: &ApplicationSpec,
) -> Result<Record, Error> {
    let updated = sqlx::query("UPDATE applications SET name=$2,owner_id=$3,active=$4,revision=$5 WHERE id=$1 AND revision=$6")
        .bind(uuid(application.as_u128()))
        .bind(spec.name.as_str())
        .bind(uuid(spec.owner.as_u128()))
        .bind(spec.active)
        .bind(integer(next_revision(revision, revision)?)?)
        .bind(integer(revision)?)
        .execute(&mut **tx)
        .await
        .map_err(constraint)?;
    if updated.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    records::application(tx, application)
        .await
        .map(Record::Application)
}
async fn update_client(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    client: ClientId,
    revision: u64,
    spec: &ClientSpec,
) -> Result<Record, Error> {
    update_client_configuration(tx, application, client, revision, spec).await?;
    records::client(tx, application, client)
        .await
        .map(Record::Client)
}
pub(super) async fn update_client_configuration(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    client: ClientId,
    revision: u64,
    spec: &ClientSpec,
) -> Result<(), Error> {
    let updated = sqlx::query(
        "UPDATE oauth_clients SET name=$2,active=$3,revision=$4,refresh_tokens=$5 WHERE id=$1 AND application_id=$6 AND revision=$7",
    )
    .bind(uuid(client.as_u128()))
    .bind(spec.name.as_str())
    .bind(spec.active)
    .bind(integer(next_revision(revision, revision)?)?)
    .bind(spec.refresh_tokens)
    .bind(uuid(application.as_u128()))
    .bind(integer(revision)?)
    .execute(&mut **tx)
    .await
    .map_err(constraint)?;
    affected(updated.rows_affected(), 1)?;
    replace_grants(tx, application, client, spec).await
}
async fn updated_client(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    client: ClientId,
    revision: u64,
) -> Result<Record, Error> {
    bump(tx, client, revision).await?;
    records::client(tx, application, client)
        .await
        .map(Record::Client)
}
async fn retire(tx: &mut Tx<'_>, client: ClientId, secret: ClientSecretId) -> Result<(), Error> {
    sqlx::query("UPDATE oauth_client_secrets SET retired=true WHERE id=$1 AND client_id=$2")
        .bind(uuid(secret.as_u128()))
        .bind(uuid(client.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
fn new_id(prepared: &Prepared) -> Result<u128, Error> {
    prepared
        .identifier
        .map(|n| n.get())
        .ok_or(Error::Unavailable)
}
async fn create_resource(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    name: &Label,
    raw: u128,
) -> Result<Record, Error> {
    let id = ResourceId::from_u128(raw).map_err(storage)?;
    let audience = format!("urn:darkhorse:resource:{}", uuid(raw));
    sqlx::query(
        "INSERT INTO protected_resources (id,application_id,name,audience) VALUES ($1,$2,$3,$4)",
    )
    .bind(uuid(raw))
    .bind(uuid(application.as_u128()))
    .bind(name.as_str())
    .bind(&audience)
    .execute(&mut **tx)
    .await
    .map_err(constraint)?;
    Ok(Record::Resource(ResourceRecord {
        id,
        application,
        name: name.as_str().into(),
        audience,
    }))
}
async fn create_scope(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    resource: ResourceId,
    name: &ScopeName,
    raw: u128,
) -> Result<Record, Error> {
    let id = ScopeId::from_u128(raw).map_err(storage)?;
    sqlx::query(
        "INSERT INTO resource_scopes (id,application_id,resource_id,name) VALUES ($1,$2,$3,$4)",
    )
    .bind(uuid(raw))
    .bind(uuid(application.as_u128()))
    .bind(uuid(resource.as_u128()))
    .bind(name.as_str())
    .execute(&mut **tx)
    .await
    .map_err(constraint)?;
    Ok(Record::Scope(ScopeRecord {
        id,
        application,
        resource,
        name: name.as_str().into(),
    }))
}
async fn create_client(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    spec: &ClientSpec,
    prepared: Prepared,
    now: u64,
) -> Result<Record, Error> {
    let id = ClientId::from_u128(new_id(&prepared)?).map_err(storage)?;
    sqlx::query("INSERT INTO oauth_clients (id,application_id,name,active,refresh_tokens) VALUES ($1,$2,$3,$4,$5)")
        .bind(uuid(id.as_u128()))
        .bind(uuid(application.as_u128()))
        .bind(spec.name.as_str())
        .bind(spec.active)
        .bind(spec.refresh_tokens)
        .execute(&mut **tx)
        .await
        .map_err(constraint)?;
    replace_grants(tx, application, id, spec).await?;
    insert_secret(tx, id, prepared.secret.ok_or(Error::Unavailable)?, now).await?;
    records::client(tx, application, id)
        .await
        .map(Record::Client)
}
async fn replace_grants(
    tx: &mut Tx<'_>,
    application: ApplicationId,
    client: ClientId,
    spec: &ClientSpec,
) -> Result<(), Error> {
    clear_grants(tx, client).await?;
    let redirects =
        sqlx::query("INSERT INTO client_redirects (client_id,uri) SELECT $1,unnest($2::text[])")
            .bind(uuid(client.as_u128()))
            .bind(spec.redirects.values())
            .execute(&mut **tx)
            .await
            .map_err(constraint)?;
    affected(
        redirects.rows_affected(),
        spec.redirects.values().len() as u64,
    )?;
    let resources = sqlx::query("INSERT INTO client_resources (application_id,client_id,resource_id) SELECT $1,$2,unnest($3::uuid[])")
        .bind(uuid(application.as_u128())).bind(uuid(client.as_u128())).bind(spec.resources.iter().map(|r|uuid(r.as_u128())).collect::<Vec<_>>()).execute(&mut **tx).await.map_err(constraint)?;
    affected(resources.rows_affected(), spec.resources.len() as u64)?;
    let scopes = sqlx::query("INSERT INTO client_scopes (application_id,client_id,resource_id,scope_id) SELECT $1,$2,resource_id,id FROM resource_scopes WHERE id=ANY($3)")
        .bind(uuid(application.as_u128())).bind(uuid(client.as_u128())).bind(spec.scopes.iter().map(|s|uuid(s.as_u128())).collect::<Vec<_>>()).execute(&mut **tx).await.map_err(constraint)?;
    affected(scopes.rows_affected(), spec.scopes.len() as u64)
}
fn affected(actual: u64, expected: u64) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}
async fn clear_grants(tx: &mut Tx<'_>, client: ClientId) -> Result<(), Error> {
    sqlx::query("DELETE FROM client_scopes WHERE client_id=$1")
        .bind(uuid(client.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("DELETE FROM client_resources WHERE client_id=$1")
        .bind(uuid(client.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("DELETE FROM client_redirects WHERE client_id=$1")
        .bind(uuid(client.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    let remaining: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM client_scopes WHERE client_id=$1 UNION ALL SELECT 1 FROM client_resources WHERE client_id=$1 UNION ALL SELECT 1 FROM client_redirects WHERE client_id=$1)")
        .bind(uuid(client.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
    if remaining {
        return Err(Error::Unavailable);
    }
    Ok(())
}
async fn insert_secret(
    tx: &mut Tx<'_>,
    client: ClientId,
    secret: SecretVerifier,
    now: u64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO oauth_client_secrets (id,client_id,verifier,created_ms) VALUES ($1,$2,$3,$4)",
    )
    .bind(uuid(secret.id.as_u128()))
    .bind(uuid(client.as_u128()))
    .bind(secret.digest.as_slice())
    .bind(integer(now)?)
    .execute(&mut **tx)
    .await
    .map_err(constraint)?;
    Ok(())
}
async fn rotate(
    tx: &mut Tx<'_>,
    client: ClientId,
    secret: SecretVerifier,
    now: u64,
    overlap: u16,
) -> Result<(), Error> {
    sqlx::query("UPDATE oauth_client_secrets SET retired=true WHERE client_id=$1 AND expires_ms IS NOT NULL AND NOT retired")
        .bind(uuid(client.as_u128())).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("UPDATE oauth_client_secrets SET expires_ms=$2 WHERE client_id=$1 AND expires_ms IS NULL AND NOT retired")
        .bind(uuid(client.as_u128())).bind(integer(overlap_deadline(now,overlap)?)?).execute(&mut **tx).await.map_err(constraint)?;
    insert_secret(tx, client, secret, now).await
}
async fn bump(tx: &mut Tx<'_>, client: ClientId, revision: u64) -> Result<(), Error> {
    sqlx::query("UPDATE oauth_clients SET revision=$2 WHERE id=$1")
        .bind(uuid(client.as_u128()))
        .bind(integer(next_revision(revision, revision)?)?)
        .execute(&mut **tx)
        .await
        .map_err(constraint)?;
    Ok(())
}
