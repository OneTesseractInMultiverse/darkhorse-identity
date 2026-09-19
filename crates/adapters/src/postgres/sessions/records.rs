use super::*;
const ACTOR: &str = "SELECT s.*,p.active,p.credential_epoch AS current_epoch,NOT c.revoked AS credential_live FROM browser_sessions s JOIN principals p ON p.id=s.principal_id JOIN credentials c ON c.id=s.credential_id AND c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE s.digest=$1 FOR SHARE OF s,p,c,pc";
pub(super) async fn actor(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<Actor, Error> {
    let row = sqlx::query(ACTOR)
        .bind(digest.as_slice())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::Unauthorized)?;
    let now = now(tx).await?;
    checked_actor(&row, now)
}
fn checked_actor(row: &PgRow, now: u64) -> Result<Actor, Error> {
    policy::authenticate(facts(row)?, now)?;
    Ok(Actor {
        id: id(row)?,
        principal: principal(row)?,
    })
}
pub(super) fn id(row: &PgRow) -> Result<SessionId, Error> {
    SessionId::from_u128(
        row.try_get::<Uuid, _>("public_id")
            .map_err(storage)?
            .as_u128(),
    )
    .map_err(storage)
}
pub(super) fn principal(row: &PgRow) -> Result<PrincipalId, Error> {
    PrincipalId::from_u128(
        row.try_get::<Uuid, _>("principal_id")
            .map_err(storage)?
            .as_u128(),
    )
    .map_err(storage)
}
fn facts(row: &PgRow) -> Result<SessionFacts, Error> {
    Ok(SessionFacts {
        active: row.try_get("active").map_err(storage)?,
        credential_live: row.try_get("credential_live").map_err(storage)?,
        revoked: row.try_get("revoked").map_err(storage)?,
        issued_epoch: number(row, "credential_epoch")?,
        current_epoch: number(row, "current_epoch")?,
        created_ms: number(row, "created_ms")?,
        seen_ms: number(row, "seen_ms")?,
        expires_ms: number(row, "expires_ms")?,
    })
}
pub(super) async fn list(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    after: Option<Cursor>,
) -> Result<Vec<PgRow>, Error> {
    let after = validated_cursor(after)?;
    sqlx::query("SELECT s.public_id,s.principal_id,s.credential_epoch,s.created_ms,s.seen_ms,s.expires_ms,s.revoked,p.active,p.credential_epoch AS current_epoch,NOT c.revoked AS credential_live FROM browser_sessions s JOIN principals p ON p.id=s.principal_id JOIN credentials c ON c.id=s.credential_id AND c.principal_id=p.id WHERE s.principal_id=$1 AND ($2::bigint IS NULL OR (s.created_ms,s.public_id)<($2,$3::uuid)) ORDER BY s.created_ms DESC,s.public_id DESC LIMIT $4")
        .bind(Uuid::from_u128(principal.as_u128())).bind(after.map(|c| c.created_ms as i64)).bind(after.map(|c| Uuid::from_u128(c.id.as_u128()))).bind((policy::PAGE_SIZE+1) as i64)
        .fetch_all(&mut **tx).await.map_err(storage)
}
fn validated_cursor(after: Option<Cursor>) -> Result<Option<Cursor>, Error> {
    after.map(|c| Cursor::new(c.created_ms, c.id)).transpose()
}
pub(super) fn page(actor: &Actor, rows: &[PgRow], now: u64) -> Result<Page, Error> {
    let items = rows
        .iter()
        .map(|row| record(row, actor.principal, now))
        .collect::<Result<Vec<_>, _>>()?;
    policy::page(actor.id, items)
}
fn record(row: &PgRow, owner: PrincipalId, now: u64) -> Result<Record, Error> {
    policy::owns(owner, principal(row)?)?;
    Ok(Record {
        id: id(row)?,
        created_ms: number(row, "created_ms")?,
        seen_ms: number(row, "seen_ms")?,
        expires_ms: number(row, "expires_ms")?,
        status: policy::status(facts(row)?, now),
    })
}
pub(super) async fn target(
    tx: &mut Tx<'_>,
    target: SessionId,
    mutation: bool,
) -> Result<PgRow, Error> {
    let query = if mutation {
        "SELECT * FROM browser_sessions WHERE public_id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM browser_sessions WHERE public_id=$1"
    };
    sqlx::query(query)
        .bind(Uuid::from_u128(target.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)
}
pub(super) async fn end(
    tx: &mut Tx<'_>,
    row: &PgRow,
    actor: SessionId,
    event: &str,
    now: u64,
) -> Result<(), Error> {
    let changed =
        sqlx::query("UPDATE browser_sessions SET revoked=true WHERE public_id=$1 AND NOT revoked")
            .bind(Uuid::from_u128(id(row)?.as_u128()))
            .execute(&mut **tx)
            .await
            .map_err(storage)?
            .rows_affected();
    if changed > 0 {
        audit(tx, row, Some(actor), event, now).await?;
    }
    Ok(())
}
pub(super) async fn audit(
    tx: &mut Tx<'_>,
    row: &PgRow,
    actor: Option<SessionId>,
    event: &str,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO session_audit(principal_id,session_id,actor_session_id,event,occurred_ms) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::from_u128(principal(row)?.as_u128())).bind(Uuid::from_u128(id(row)?.as_u128())).bind(actor.map(|id|Uuid::from_u128(id.as_u128()))).bind(event).bind(now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
