use super::*;

pub(crate) async fn enqueue(
    tx: &mut Tx<'_>,
    principal: Uuid,
    invalidated_epoch: Option<u64>,
) -> Result<(), sqlx::Error> {
    let Some(epoch) = invalidated_epoch else {
        return Ok(());
    };
    let inserted = sqlx::query("INSERT INTO logout_account_intents(principal_id,credential_epoch,created_ms) VALUES($1,$2,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
        .bind(principal).bind(epoch as i64).execute(&mut **tx).await?;
    one(inserted.rows_affected())
}

pub(super) async fn expand(tx: &mut Tx<'_>) -> Result<(), sqlx::Error> {
    let root = sqlx::query("SELECT * FROM logout_account_intents WHERE NOT expanded ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 1")
        .fetch_optional(&mut **tx).await?;
    let Some(root) = root else {
        return Ok(());
    };
    let result = sqlx::query(include_str!("accounts-expand.sql"))
        .bind(root.try_get::<Uuid, _>("principal_id")?)
        .bind(root.try_get::<i64, _>("credential_epoch")?)
        .bind(root.try_get::<Option<Uuid>, _>("cursor")?)
        .bind(root.try_get::<i64, _>("id")?)
        .bind(root.try_get::<i64, _>("created_ms")?)
        .fetch_one(&mut **tx)
        .await?;
    if result.try_get::<i64, _>("eligible")? != result.try_get::<i64, _>("inserted")? {
        return Err(sqlx::Error::RowNotFound);
    }
    let changed = sqlx::query(
        "UPDATE logout_account_intents SET cursor=COALESCE($2,cursor),expanded=$3 WHERE id=$1",
    )
    .bind(root.try_get::<i64, _>("id")?)
    .bind(result.try_get::<Option<Uuid>, _>("cursor")?)
    .bind(result.try_get::<i64, _>("targets")? < 100)
    .execute(&mut **tx)
    .await?;
    one(changed.rows_affected())
}

pub(super) async fn cleanup(tx: &mut Tx<'_>, cutoff: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM logout_account_intents WHERE id IN (SELECT id FROM logout_account_intents WHERE expanded AND created_ms<$1 ORDER BY created_ms,id LIMIT 100 FOR UPDATE SKIP LOCKED)")
        .bind(cutoff).execute(&mut **tx).await?;
    Ok(())
}
