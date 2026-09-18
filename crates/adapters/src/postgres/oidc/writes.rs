use super::*;
pub(super) async fn capacity(tx: &mut Tx<'_>, client: ClientId) -> Result<(), Error> {
    sqlx::query("SELECT singleton FROM authorization_capacity WHERE singleton FOR UPDATE")
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
    let now = authority::now(tx).await?;
    sqlx::query("DELETE FROM authorization_requests WHERE digest IN (SELECT digest FROM authorization_requests WHERE expires_ms<=$1 ORDER BY expires_ms LIMIT 100)").bind(integer(now)?).execute(&mut **tx).await.map_err(storage)?;
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER(WHERE client_id=$1) FROM authorization_requests",
    )
    .bind(Uuid::from_u128(client.as_u128()))
    .fetch_one(&mut **tx)
    .await
    .map_err(storage)?;
    darkhorse_domain::oidc::capacity(
        counts.0.try_into().map_err(storage)?,
        counts.1.try_into().map_err(storage)?,
    )
}
pub(super) async fn insert(
    tx: &mut Tx<'_>,
    r: &Request,
    p: &ClientPolicy,
    handle: [u8; 32],
    initial: Option<[u8; 32]>,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO authorization_requests(digest,client_id,client_revision,application_revision,redirect_uri,challenge,state,nonce,scopes,resource,prompt,max_age,created_ms,expires_ms,initial_session,resource_policy_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,CASE WHEN $10::text IS NULL THEN NULL ELSE (SELECT policy_revision FROM security_state WHERE singleton) END)")
  .bind(handle.as_slice()).bind(Uuid::from_u128(r.client.as_u128())).bind(integer(p.revision)?).bind(integer(p.application_revision)?).bind(&r.redirect).bind(r.challenge.as_slice()).bind(&r.state).bind(&r.nonce).bind(&r.scopes).bind(&r.resource).bind(records::prompt(r.prompt)).bind(r.max_age.map(integer).transpose()?).bind(integer(now)?).bind(integer(now.checked_add(TRANSACTION_MS).ok_or(Error::Unavailable)?)?).bind(initial.as_ref().map(|d|d.as_slice()))
  .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn bind(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    current: Option<Session>,
    interaction: Interaction,
) -> Result<(), Error> {
    if interaction == Interaction::Login {
        return Ok(());
    }
    let session = current.ok_or(Error::InvalidTransaction)?;
    sqlx::query("UPDATE authorization_requests SET bound_session=$2,principal_id=$3,authenticated_ms=$4 WHERE digest=$1 AND bound_session IS NULL")
  .bind(handle.as_slice()).bind(session.digest.as_slice()).bind(Uuid::from_u128(session.principal.as_u128())).bind(integer(session.authenticated_ms)?).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn approve(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    request: &Request,
    policy: &ClientPolicy,
    session: Session,
) -> Result<(), Error> {
    let ceiling = super::super::resource_authority::approve(tx, handle, request, session)
        .await
        .map_err(|error| match error {
            darkhorse_domain::tokens::Error::Unavailable => Error::Unavailable,
            _ => Error::AccessDenied,
        })?;
    sqlx::query("INSERT INTO oauth_consents(principal_id,client_id,resource,client_revision,application_revision,scopes) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(principal_id,client_id,resource) DO UPDATE SET client_revision=EXCLUDED.client_revision,application_revision=EXCLUDED.application_revision,scopes=EXCLUDED.scopes")
  .bind(Uuid::from_u128(session.principal.as_u128())).bind(Uuid::from_u128(request.client.as_u128())).bind(request.resource.as_deref().unwrap_or(""))
  .bind(integer(policy.revision)?).bind(integer(policy.application_revision)?).bind(&request.scopes).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("INSERT INTO consent_audit(principal_id,client_id,resource,scopes,capability_ceiling,occurred_ms) VALUES($1,$2,$3,$4,$5,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
        .bind(Uuid::from_u128(session.principal.as_u128())).bind(Uuid::from_u128(request.client.as_u128())).bind(request.resource.as_deref().unwrap_or("")).bind(&request.scopes).bind(&ceiling).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("UPDATE authorization_requests SET approved=true WHERE digest=$1")
        .bind(handle.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
pub(super) async fn finish(tx: &mut Tx<'_>, handle: [u8; 32]) -> Result<(), Error> {
    sqlx::query("UPDATE authorization_requests SET terminal=true WHERE digest=$1")
        .bind(handle.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
