use super::*;
pub(super) struct Prepared {
    pub grants: Vec<Grant>,
    pub expiry: ExpiryPolicy,
}
pub(super) async fn actor(tx: &mut Tx<'_>, digest: [u8; 32], recent: bool) -> Result<Actor, Error> {
    let (principal, session) = sessions::owner(tx, digest).await.map_err(session_error)?;
    let row=sqlx::query("SELECT s.created_ms,p.credential_epoch FROM browser_sessions s JOIN principals p ON p.id=s.principal_id WHERE s.public_id=$1")
        .bind(uuid(session.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
    let now = sessions::now(tx).await.map_err(session_error)?;
    if recent {
        policy::recent(number(&row, "created_ms")?, now)?;
    }
    Ok(Actor {
        principal,
        session,
        epoch: number(&row, "credential_epoch")?,
        now,
    })
}
pub(super) async fn policy(tx: &mut Tx<'_>) -> Result<(ExpiryPolicy, u64), Error> {
    let row=sqlx::query("SELECT default_days,maximum_days,allow_never,policy_revision FROM personal_key_policy CROSS JOIN security_state")
        .fetch_one(&mut **tx).await.map_err(storage)?;
    Ok((
        ExpiryPolicy::new(
            row.try_get::<i16, _>("default_days")
                .map_err(storage)?
                .try_into()
                .map_err(storage)?,
            row.try_get::<i16, _>("maximum_days")
                .map_err(storage)?
                .try_into()
                .map_err(storage)?,
            row.try_get("allow_never").map_err(storage)?,
        )
        .map_err(storage)?,
        number(&row, "policy_revision")?,
    ))
}
pub(super) async fn prepare(
    tx: &mut Tx<'_>,
    actor: &Actor,
    request: &Request,
) -> Result<Prepared, Error> {
    let (expiry, revision) = policy(tx).await?;
    policy::revision(revision, request.revision())?;
    expiry.deadline(request.expiration(), actor.now)?;
    capacity(tx, actor).await?;
    let mut grants = Vec::new();
    for selection in request.grants() {
        let policy =
            projection::load(tx, actor.principal, selection.resource, &BTreeSet::new()).await?;
        let plan = policy.plan(request.application(), selection.capabilities.clone())?;
        grants.push(Grant {
            resource: selection.resource,
            ceiling: plan.ceiling,
        });
    }
    Ok(Prepared { grants, expiry })
}
async fn capacity(tx: &mut Tx<'_>, actor: &Actor) -> Result<(), Error> {
    let (live,recent):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM (SELECT 1 FROM personal_keys k JOIN credentials c ON c.id=k.credential_id WHERE k.principal_id=$1 AND NOT c.revoked AND k.principal_epoch=$2 AND (k.expires_ms IS NULL OR k.expires_ms>$3) LIMIT 100) live),(SELECT count(*) FROM (SELECT 1 FROM personal_keys WHERE principal_id=$1 AND created_ms>$3-600000 LIMIT 10) recent)")
        .bind(uuid(actor.principal.as_u128())).bind(actor.epoch as i64).bind(actor.now as i64).fetch_one(&mut **tx).await.map_err(storage)?;
    policy::capacity(
        live.try_into().map_err(storage)?,
        recent.try_into().map_err(storage)?,
    )
}
