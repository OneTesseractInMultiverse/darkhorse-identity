use super::*;
pub(super) async fn family(
    tx: &mut Tx<'_>,
    code: [u8; 32],
    issuer: &str,
    family: &Family,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO refresh_families(code_digest,issuer,created_ms,expires_ms) VALUES($1,$2,$3,$4)")
        .bind(code.as_slice()).bind(issuer).bind(family.created as i64).bind(family.expires as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn owned(
    tx: &mut Tx<'_>,
    token: [u8; 32],
    client: ClientId,
    issuer: &str,
) -> Result<[u8; 32], Error> {
    let digest:Vec<u8> = sqlx::query_scalar("SELECT r.code_digest FROM refresh_tokens r JOIN authorization_codes c ON c.digest=r.code_digest JOIN refresh_families f ON f.code_digest=r.code_digest WHERE r.digest=$1 AND c.client_id=$2 AND f.issuer=$3")
        .bind(token.as_slice()).bind(Uuid::from_u128(client.as_u128())).bind(issuer)
        .fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidGrant)?;
    digest.try_into().map_err(storage)
}
pub(super) async fn load(tx: &mut Tx<'_>, token: [u8; 32]) -> Result<Stored, Error> {
    let row = sqlx::query("SELECT r.*, f.created_ms AS family_created,f.expires_ms AS family_expires,f.revoked FROM refresh_tokens r JOIN refresh_families f USING(code_digest) WHERE r.digest=$1")
        .bind(token.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidGrant)?;
    decode(&row)
}
fn decode(row: &PgRow) -> Result<Stored, Error> {
    Ok(Stored {
        family: Family {
            created: number(row, "family_created")?,
            expires: number(row, "family_expires")?,
            revoked: row.try_get("revoked").map_err(storage)?,
        },
        member: Member {
            created: number(row, "created_ms")?,
            expires: number(row, "expires_ms")?,
            generation: row
                .try_get::<i16, _>("generation")
                .map_err(storage)?
                .try_into()
                .map_err(storage)?,
            consumed: row.try_get("consumed").map_err(storage)?,
        },
        scopes: row
            .try_get::<String, _>("scope")
            .map_err(storage)?
            .split(' ')
            .map(String::from)
            .collect(),
        ceiling: crate::postgres::resource_authority::capabilities(
            &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
                .map_err(storage)?,
        )?,
    })
}
pub(super) async fn consume(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<(), Error> {
    let count =
        sqlx::query("UPDATE refresh_tokens SET consumed=true WHERE digest=$1 AND NOT consumed")
            .bind(digest.as_slice())
            .execute(&mut **tx)
            .await
            .map_err(storage)?
            .rows_affected();
    if count != 1 {
        return Err(Error::InvalidGrant);
    }
    Ok(())
}
pub(super) async fn insert(
    tx: &mut Tx<'_>,
    code: [u8; 32],
    token: [u8; 32],
    grant: &IssuedGrant,
    now: u64,
    window: Window,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO refresh_tokens(digest,code_digest,generation,scope,capability_ceiling,created_ms,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(token.as_slice()).bind(code.as_slice()).bind(window.generation as i16).bind(&grant.scope).bind(&grant.capabilities).bind(now as i64).bind(window.refresh_expires as i64)
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn audit(
    tx: &mut Tx<'_>,
    code: [u8; 32],
    generation: u16,
    event: &str,
    now: u64,
) -> Result<(), Error> {
    let count = sqlx::query("INSERT INTO token_audit(principal_id,client_id,event,occurred_ms,refresh_family_id,refresh_generation) SELECT c.principal_id,c.client_id,$2,$3,f.id,$4 FROM authorization_codes c JOIN refresh_families f ON f.code_digest=c.digest WHERE c.digest=$1")
        .bind(code.as_slice()).bind(event).bind(now as i64).bind(generation as i16)
        .execute(&mut **tx).await.map_err(storage)?.rows_affected();
    if count != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
pub(super) async fn revoke(
    tx: &mut Tx<'_>,
    code: [u8; 32],
    generation: u16,
    event: &str,
    now: u64,
) -> Result<(), Error> {
    let changed = sqlx::query(
        "UPDATE refresh_families SET revoked=true WHERE code_digest=$1 AND NOT revoked",
    )
    .bind(code.as_slice())
    .execute(&mut **tx)
    .await
    .map_err(storage)?
    .rows_affected();
    if changed > 0 {
        audit(tx, code, generation, event, now).await?;
    }
    Ok(())
}
