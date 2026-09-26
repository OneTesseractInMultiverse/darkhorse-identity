use super::*;
pub(in crate::postgres) async fn catalog(
    tx: &mut Tx<'_>,
    client: ClientId,
) -> Result<Catalog, Error> {
    let id = Uuid::from_u128(client.as_u128());
    let row=sqlx::query("SELECT c.name,c.active AND a.active AS active,c.revision,a.revision AS application_revision FROM oauth_clients c JOIN applications a ON a.id=c.application_id WHERE c.id=$1")
  .bind(id).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidRequest)?;
    let redirects: Vec<String> = sqlx::query_scalar(
        "SELECT uri FROM client_redirects WHERE client_id=$1 ORDER BY uri LIMIT 9",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    let resources=sqlx::query("SELECT r.audience,ARRAY(SELECT s.name FROM client_scopes cs JOIN resource_scopes s ON s.id=cs.scope_id WHERE cs.client_id=$1 AND cs.resource_id=r.id ORDER BY s.name LIMIT 129) AS scopes FROM client_resources cr JOIN protected_resources r ON r.id=cr.resource_id WHERE cr.client_id=$1 ORDER BY r.audience LIMIT 33")
  .bind(id).fetch_all(&mut **tx).await.map_err(storage)?;
    let resources = resources
        .iter()
        .map(resource)
        .collect::<Result<Vec<_>, _>>()?;
    if redirects.len() > 8 || resources.len() > 32 {
        return Err(Error::Unavailable);
    }
    Ok(Catalog {
        name: row.try_get("name").map_err(storage)?,
        policy: ClientPolicy {
            active: row.try_get("active").map_err(storage)?,
            revision: number(&row, "revision")?,
            application_revision: number(&row, "application_revision")?,
            redirects,
            resources,
        },
    })
}
fn resource(row: &PgRow) -> Result<(String, Vec<String>), Error> {
    let scopes: Vec<String> = row.try_get("scopes").map_err(storage)?;
    if scopes.len() > 128 {
        return Err(Error::Unavailable);
    }
    Ok((row.try_get("audience").map_err(storage)?, scopes))
}
pub(super) async fn pending(tx: &mut Tx<'_>, handle: [u8; 32]) -> Result<Pending, Error> {
    let row = sqlx::query(
        "SELECT * FROM authorization_requests WHERE digest=$1 AND NOT terminal FOR UPDATE",
    )
    .bind(handle.as_slice())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?
    .ok_or(Error::InvalidTransaction)?;
    decode(&row)
}
fn decode(row: &PgRow) -> Result<Pending, Error> {
    let request = Request {
        client: ClientId::from_u128(
            row.try_get::<Uuid, _>("client_id")
                .map_err(storage)?
                .as_u128(),
        )
        .map_err(storage)?,
        redirect: row.try_get("redirect_uri").map_err(storage)?,
        challenge: row
            .try_get::<Vec<u8>, _>("challenge")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        state: row.try_get("state").map_err(storage)?,
        nonce: row.try_get("nonce").map_err(storage)?,
        scopes: row.try_get("scopes").map_err(storage)?,
        resource: row.try_get("resource").map_err(storage)?,
        prompt: parse_prompt(&row.try_get::<String, _>("prompt").map_err(storage)?)?,
        ui_locale: row
            .try_get::<Option<String>, _>("ui_locale")
            .map_err(storage)?
            .as_deref()
            .map(crate::localization::parse)
            .transpose()
            .map_err(storage)?,
        max_age: row
            .try_get::<Option<i64>, _>("max_age")
            .map_err(storage)?
            .map(|n| n.try_into().map_err(storage))
            .transpose()?,
    };
    request.validate()?;
    Ok(Pending {
        request,
        created: number(row, "created_ms")?,
        initial: digest(row, "initial_session")?,
        bound: bound(row)?,
        approved: row.try_get("approved").map_err(storage)?,
        client_revision: number(row, "client_revision")?,
        application_revision: number(row, "application_revision")?,
    })
}
fn bound(row: &PgRow) -> Result<Option<Session>, Error> {
    let Some(digest) = digest(row, "bound_session")? else {
        return Ok(None);
    };
    Ok(Some(Session {
        digest,
        principal: PrincipalId::from_u128(
            row.try_get::<Uuid, _>("principal_id")
                .map_err(storage)?
                .as_u128(),
        )
        .map_err(storage)?,
        authenticated_ms: number(row, "authenticated_ms")?,
    }))
}
fn parse_prompt(value: &str) -> Result<Prompt, Error> {
    match value {
        "default" => Ok(Prompt::Default),
        "none" => Ok(Prompt::None),
        "login" => Ok(Prompt::Login),
        "consent" => Ok(Prompt::Consent),
        "login_consent" => Ok(Prompt::LoginConsent),
        _ => Err(Error::Unavailable),
    }
}
pub(super) fn prompt(value: Prompt) -> &'static str {
    match value {
        Prompt::Default => "default",
        Prompt::None => "none",
        Prompt::Login => "login",
        Prompt::Consent => "consent",
        Prompt::LoginConsent => "login_consent",
    }
}
