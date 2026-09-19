use super::*;
pub(super) async fn invite(
    tx: &mut Tx<'_>,
    issuer: PrincipalId,
    actor: [u8; 32],
    email: &str,
    material: &Material,
    now: u64,
) -> Result<(), Error> {
    let key = email.to_ascii_lowercase();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM principals WHERE email_key=$1)")
            .bind(&key)
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
    policy::ensure_new_recipient(exists)?;
    let counts = sqlx::query("SELECT (SELECT max(created_ms) FROM invitations WHERE email_key=$1) AS last_ms,(SELECT count(*) FROM invitations WHERE email_key=$1 AND created_ms>$3) AS recipient_daily,(SELECT count(*) FROM invitations WHERE issuer_id=$2 AND created_ms>$3) AS actor_daily,((SELECT count(*) FROM invitations WHERE delivery_state='queued' AND expires_ms>$4)+(SELECT count(*) FROM email_verifications WHERE delivery_state='queued' AND expires_ms>$4)) AS queued")
 .bind(&key)
        .bind(uuid(issuer.as_u128()))
        .bind(now.saturating_sub(policy::LIFETIME_MS) as i64)
        .bind(now as i64)
        .fetch_one(&mut **tx).await.map_err(storage)?;
    let expires = expiry(&counts, now)?;
    sqlx::query("UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE email_key=$1 AND NOT closed")
        .bind(&key)
        .execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("INSERT INTO invitations(id,email,issuer_id,issuer_credential_id,issuer_epoch,digest,seed,created_ms,expires_ms,next_ms) SELECT $1,$2,s.principal_id,s.credential_id,s.credential_epoch,$4,$5,$6,$7,$6 FROM browser_sessions s WHERE s.digest=$3")
 .bind(uuid(material.id.as_u128()))
        .bind(email)
        .bind(actor.as_slice())
        .bind(material.digest.as_slice())
        .bind(material.seed.as_slice())
        .bind(now as i64)
        .bind(expires as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    audit(tx, material.id, Some(issuer), "invited", 0, now).await
}
fn expiry(row: &PgRow, now: u64) -> Result<u64, Error> {
    let last: Option<i64> = row.try_get("last_ms").map_err(storage)?;
    policy::issue(
        last.map(|n| n as u64),
        number(row, "recipient_daily")?,
        number(row, "actor_daily")?,
        number(row, "queued")?,
        now,
    )
}
pub(super) async fn revoke(
    tx: &mut Tx<'_>,
    id: InvitationId,
    actor: PrincipalId,
    now: u64,
) -> Result<(), Error> {
    let row = sqlx::query("UPDATE invitations SET closed=true,seed=NULL,delivery_state='cancelled' WHERE id=$1 AND NOT closed RETURNING id")
        .bind(uuid(id.as_u128()))
        .fetch_optional(&mut **tx).await.map_err(storage)?;
    if row.is_some() {
        audit(tx, id, Some(actor), "revoked", 0, now).await?;
    }
    Ok(())
}
pub(super) async fn admit(tx: &mut Tx<'_>, row: &PgRow, now: u64) -> Result<(), Error> {
    let global: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invitation_audit WHERE event='hash_admitted' AND occurred_ms>$1",
    )
    .bind(now.saturating_sub(60_000) as i64)
    .fetch_one(&mut **tx)
    .await
    .map_err(storage)?;
    let attempts: i16 = row.try_get("hash_attempts").map_err(storage)?;
    policy::admit(attempts as u64, global as u64)?;
    sqlx::query("UPDATE invitations SET hash_attempts=hash_attempts+1 WHERE id=$1")
        .bind(uuid(id(row)?.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    audit(
        tx,
        id(row)?,
        None,
        "hash_admitted",
        (attempts + 1) as u16,
        now,
    )
    .await
}
pub(super) async fn create(
    tx: &mut Tx<'_>,
    profile: &Profile,
    credential: &PreparedCredential,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO principals(id,email,first_name,last_name,email_verified_ms) VALUES($1,$2,$3,$4,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
 .bind(uuid(credential.principal_id.as_u128()))
        .bind(profile.email())
        .bind(profile.first_name())
        .bind(profile.last_name())
        .execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("INSERT INTO credentials(id,principal_id,kind) VALUES($1,$2,'password')")
        .bind(uuid(credential.credential_id.as_u128()))
        .bind(uuid(credential.principal_id.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("INSERT INTO password_credentials(credential_id,verifier) VALUES($1,$2)")
        .bind(uuid(credential.credential_id.as_u128()))
        .bind(&credential.verifier)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
pub(super) async fn consume(
    tx: &mut Tx<'_>,
    id: InvitationId,
    principal: PrincipalId,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("UPDATE invitations SET closed=true,principal_id=$2,seed=NULL,delivery_state=CASE WHEN delivery_state='queued' THEN 'cancelled' ELSE delivery_state END WHERE id=$1")
        .bind(uuid(id.as_u128()))
        .bind(uuid(principal.as_u128()))
        .execute(&mut **tx).await.map_err(storage)?;
    audit(tx, id, Some(principal), "created", 0, now).await
}
