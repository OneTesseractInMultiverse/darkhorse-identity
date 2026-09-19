use super::*;
pub(super) async fn preflight(
    store: &PostgresStore,
    digest: [u8; 32],
    email: &str,
) -> Result<(), Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    lock(&mut tx, false).await?;
    let row = proof(&mut tx, digest, email).await?;
    check(&row, now(&mut tx).await?)?;
    tx.commit().await.map_err(storage)
}
pub(super) async fn proof(tx: &mut Tx<'_>, digest: [u8; 32], email: &str) -> Result<PgRow, Error> {
    sqlx::query("SELECT i.*, (p.active AND p.credential_epoch=i.issuer_epoch AND NOT c.revoked AND EXISTS(SELECT 1 FROM platform_administrators a WHERE a.principal_id=p.id)) AS eligible, EXISTS(SELECT 1 FROM principals recipient WHERE recipient.email_key=i.email_key) AS account_exists FROM invitations i JOIN principals p ON p.id=i.issuer_id JOIN credentials c ON c.id=i.issuer_credential_id AND c.principal_id=p.id JOIN password_credentials pc ON pc.credential_id=c.id WHERE i.digest=$1 AND i.email_key=$2 FOR SHARE OF i,p,c,pc")
 .bind(digest.as_slice())
        .bind(email)
        .fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::Invalid)
}
pub(super) fn check(row: &PgRow, now: u64) -> Result<(), Error> {
    policy::redeem(
        &policy::Proof {
            created_ms: number(row, "created_ms")?,
            expires_ms: number(row, "expires_ms")?,
            closed: row.try_get("closed").map_err(storage)?,
            issuer_eligible: row.try_get("eligible").map_err(storage)?,
            account_exists: row.try_get("account_exists").map_err(storage)?,
        },
        now,
    )
}
pub(super) fn acceptance(row: &PgRow, expected: InvitationId, now: u64) -> Result<(), Error> {
    check(row, now)?;
    if id(row)? != expected || row.try_get::<i16, _>("hash_attempts").map_err(storage)? == 0 {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub(super) async fn list(tx: &mut Tx<'_>, now: u64) -> Result<Vec<Record>, Error> {
    let rows = sqlx::query("SELECT * FROM invitations ORDER BY created_ms DESC,id DESC LIMIT 100")
        .fetch_all(&mut **tx)
        .await
        .map_err(storage)?;
    rows.iter().map(|row| record(row, now)).collect()
}
fn record(row: &PgRow, now: u64) -> Result<Record, Error> {
    Ok(Record {
        id: id(row)?,
        email: row.try_get("email").map_err(storage)?,
        created_ms: number(row, "created_ms")?,
        expires_ms: number(row, "expires_ms")?,
        closed: row.try_get::<bool, _>("closed").map_err(storage)?
            || now >= number(row, "expires_ms")?,
        delivery: row.try_get("delivery_state").map_err(storage)?,
    })
}
