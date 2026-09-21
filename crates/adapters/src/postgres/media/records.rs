use super::*;
pub(super) async fn current(
    tx: &mut Tx<'_>,
    target: Target,
) -> Result<(u64, Option<AssetId>), Error> {
    let row=match target.kind {
 Kind::Portrait=>sqlx::query("SELECT revision,portrait_id AS asset FROM principals WHERE id=$1 FOR SHARE").bind(target.principal.map(|id|uuid(id.as_u128()))).fetch_optional(&mut **tx).await,
 Kind::Logo=>sqlx::query("SELECT revision,logo_id AS asset FROM branding_settings WHERE singleton FOR SHARE").fetch_optional(&mut **tx).await,
 Kind::Background=>sqlx::query("SELECT revision,background_id AS asset FROM branding_settings WHERE singleton FOR SHARE").fetch_optional(&mut **tx).await,
 }.map_err(storage)?.ok_or(Error::NotFound)?;
    Ok((
        number(&row, "revision")?,
        row.try_get::<Option<Uuid>, _>("asset")
            .map_err(storage)?
            .map(|id| AssetId::from_u128(id.as_u128()).map_err(storage))
            .transpose()?,
    ))
}
pub(super) async fn budget(tx: &mut Tx<'_>, actor: PrincipalId, now: u64) -> Result<(), Error> {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM media_assets WHERE actor_id=$1 AND created_ms>$2-60000 LIMIT 4) recent").bind(uuid(actor.as_u128())).bind(now as i64).fetch_one(&mut **tx).await.map_err(storage)?;
    policy::capacity(count.try_into().map_err(storage)?)
}
pub(super) async fn reserve(
    tx: &mut Tx<'_>,
    id: AssetId,
    target: Target,
    actor: &profiles::Actor,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO media_assets(id,kind,principal_id,actor_id,created_ms,cleanup_after_ms) VALUES($1,$2,$3,$4,$5,$5+$6)").bind(uuid(id.as_u128())).bind(kind(target.kind)).bind(target.principal.map(|id|uuid(id.as_u128()))).bind(uuid(actor.principal.as_u128())).bind(actor.now as i64).bind(policy::CLEANUP_GRACE_MS as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn validate_ticket(
    tx: &mut Tx<'_>,
    ticket: &Ticket,
    actor: &profiles::Actor,
) -> Result<(), Error> {
    let row=sqlx::query("SELECT created_ms,state FROM media_assets WHERE id=$1 AND actor_id=$2 AND kind=$3 AND principal_id IS NOT DISTINCT FROM $4").bind(uuid(ticket.id.as_u128())).bind(uuid(actor.principal.as_u128())).bind(kind(ticket.target.kind)).bind(ticket.target.principal.map(|id|uuid(id.as_u128()))).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::NotFound)?;
    policy::attachable(
        number(&row, "created_ms")?,
        actor.now,
        row.try_get("state").map_err(storage)?,
    )
}
pub(super) async fn publish(tx: &mut Tx<'_>, id: AssetId, asset: &Prepared) -> Result<(), Error> {
    sqlx::query(
        "UPDATE media_assets SET state='ready',bytes=$2,digest=$3,width=$4,height=$5 WHERE id=$1",
    )
    .bind(uuid(id.as_u128()))
    .bind(asset.bytes.len() as i32)
    .bind(asset.digest.as_slice())
    .bind(asset.width as i32)
    .bind(asset.height as i32)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}
pub(super) async fn set(
    tx: &mut Tx<'_>,
    target: Target,
    id: Option<AssetId>,
    revision: u64,
) -> Result<(), Error> {
    let asset = id.map(|id| uuid(id.as_u128()));
    match target.kind {
        Kind::Portrait => {
            sqlx::query("UPDATE principals SET portrait_id=$1,revision=$2 WHERE id=$3")
                .bind(asset)
                .bind(revision as i64)
                .bind(target.principal.map(|id| uuid(id.as_u128())))
                .execute(&mut **tx)
                .await
        }
        Kind::Logo => {
            sqlx::query("UPDATE branding_settings SET logo_id=$1,revision=$2 WHERE singleton")
                .bind(asset)
                .bind(revision as i64)
                .execute(&mut **tx)
                .await
        }
        Kind::Background => {
            sqlx::query("UPDATE branding_settings SET background_id=$1,revision=$2 WHERE singleton")
                .bind(asset)
                .bind(revision as i64)
                .execute(&mut **tx)
                .await
        }
    }
    .map_err(storage)?;
    Ok(())
}
pub(super) async fn retire(tx: &mut Tx<'_>, id: Option<AssetId>, now: u64) -> Result<(), Error> {
    sqlx::query("UPDATE media_assets SET state='retired',cleanup_after_ms=$2 WHERE id=$1")
        .bind(id.map(|id| uuid(id.as_u128())))
        .bind(now as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
pub(super) async fn asset(
    tx: &mut Tx<'_>,
    id: Option<AssetId>,
    target: Target,
) -> Result<Option<Asset>, Error> {
    let Some(id) = id else {
        return Ok(None);
    };
    let row=sqlx::query("SELECT bytes,digest FROM media_assets WHERE id=$1 AND state='ready' AND kind=$2 AND principal_id IS NOT DISTINCT FROM $3").bind(uuid(id.as_u128())).bind(kind(target.kind)).bind(target.principal.map(|id|uuid(id.as_u128()))).fetch_one(&mut **tx).await.map_err(storage)?;
    Ok(Some(Asset {
        id,
        bytes: row
            .try_get::<i32, _>("bytes")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        digest: row
            .try_get::<Vec<u8>, _>("digest")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
    }))
}
pub(super) async fn audit(
    tx: &mut Tx<'_>,
    actor: &profiles::Actor,
    target: Target,
    id: Option<AssetId>,
    revision: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO media_audit(actor_id,actor_session_id,kind,principal_id,asset_id,event,target_revision,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(uuid(actor.principal.as_u128())).bind(uuid(actor.session.as_u128())).bind(kind(target.kind)).bind(target.principal.map(|id|uuid(id.as_u128()))).bind(id.map(|id|uuid(id.as_u128()))).bind(if id.is_some(){"published"}else{"removed"}).bind(revision as i64).bind(actor.now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn garbage(store: &PostgresStore) -> Result<Vec<AssetId>, Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    profiles::lock(&mut tx, true).await?;
    let now = sessions::now(&mut tx).await.map_err(storage)?;
    let rows:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM media_assets a WHERE state<>'ready' AND cleanup_after_ms<=$1 AND NOT EXISTS(SELECT 1 FROM principals p WHERE p.portrait_id=a.id) AND NOT EXISTS(SELECT 1 FROM branding_settings b WHERE b.logo_id=a.id OR b.background_id=a.id) ORDER BY cleanup_after_ms,id LIMIT 32 FOR UPDATE").bind(now as i64).fetch_all(&mut *tx).await.map_err(storage)?;
    sqlx::query(
        "UPDATE media_assets SET state='retired',cleanup_after_ms=$2+60000 WHERE id=ANY($1)",
    )
    .bind(&rows)
    .bind(now as i64)
    .execute(&mut *tx)
    .await
    .map_err(storage)?;
    tx.commit().await.map_err(storage)?;
    rows.into_iter()
        .map(|id| AssetId::from_u128(id.as_u128()).map_err(storage))
        .collect()
}
pub(super) async fn cleaned(store: &PostgresStore, id: AssetId) -> Result<(), Error> {
    // Tombstones are retained and revisited: a delayed remote PUT must not create a permanent orphan.
    sqlx::query("UPDATE media_assets SET cleanup_after_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+86400000 WHERE id=$1 AND state='retired'").bind(uuid(id.as_u128())).execute(&store.pool).await.map_err(storage)?;
    Ok(())
}
