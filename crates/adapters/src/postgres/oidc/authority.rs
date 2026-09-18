use super::*;
use darkhorse_domain::authentication::{SessionFacts, session_live};
pub(in crate::postgres) async fn lock(tx: &mut Tx<'_>) -> Result<(), Error> {
    sqlx::query("SELECT singleton FROM security_state WHERE singleton AND NOT pg_is_in_recovery() FOR SHARE").fetch_one(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(in crate::postgres) async fn now(tx: &mut Tx<'_>) -> Result<u64, Error> {
    sqlx::query_scalar::<_, i64>("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
pub(in crate::postgres) async fn session(
    tx: &mut Tx<'_>,
    digest: Option<[u8; 32]>,
) -> Result<Option<Session>, Error> {
    let Some(digest) = digest else {
        return Ok(None);
    };
    let row=sqlx::query("SELECT s.*,p.active,p.credential_epoch AS current_epoch,NOT c.revoked AS credential_live FROM browser_sessions s JOIN principals p ON p.id=s.principal_id JOIN credentials c ON c.id=s.credential_id AND c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE s.digest=$1 FOR SHARE OF s,p,c,pc")
  .bind(digest.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?;
    let now = now(tx).await?;
    row.map(|row| checked_session(&row, digest, now))
        .transpose()
        .map(Option::flatten)
}
fn checked_session(row: &PgRow, digest: [u8; 32], now: u64) -> Result<Option<Session>, Error> {
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
    if !session_live(facts, now) {
        return Ok(None);
    }
    Ok(Some(Session {
        digest,
        principal: PrincipalId::from_u128(
            row.try_get::<Uuid, _>("principal_id")
                .map_err(storage)?
                .as_u128(),
        )
        .map_err(storage)?,
        authenticated_ms: facts.created_ms,
    }))
}
pub(in crate::postgres) async fn consent(
    tx: &mut Tx<'_>,
    request: &Request,
    policy: &ClientPolicy,
    session: Option<Session>,
) -> Result<bool, Error> {
    if request.resource.is_some() {
        return Ok(false);
    }
    let Some(session) = session else {
        return Ok(false);
    };
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_consents WHERE principal_id=$1 AND client_id=$2 AND resource=$3 AND client_revision=$4 AND application_revision=$5 AND scopes @> $6::text[])")
  .bind(Uuid::from_u128(session.principal.as_u128())).bind(Uuid::from_u128(request.client.as_u128())).bind(request.resource.as_deref().unwrap_or(""))
  .bind(integer(policy.revision)?).bind(integer(policy.application_revision)?).bind(&request.scopes).fetch_one(&mut **tx).await.map_err(storage)
}
pub(super) fn unchanged(pending: &Pending, policy: &ClientPolicy) -> Result<(), Error> {
    if pending.client_revision != policy.revision
        || pending.application_revision != policy.application_revision
    {
        return Err(Error::InvalidTransaction);
    }
    authorize_request(&pending.request, policy).map_err(|_| Error::InvalidTransaction)
}
