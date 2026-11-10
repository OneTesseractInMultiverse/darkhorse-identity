use super::*;

pub(super) async fn expand(tx: &mut Tx<'_>) -> Result<(), sqlx::Error> {
    let root = sqlx::query("SELECT i.id,i.cursor,e.principal_id,e.credential_id,e.credential_epoch,e.created_ms FROM logout_credential_intents i JOIN credential_logout_events e ON e.ordering=i.id WHERE NOT i.expanded ORDER BY i.id FOR UPDATE OF i SKIP LOCKED LIMIT 1")
        .fetch_optional(&mut **tx).await?;
    let Some(root) = root else {
        return Ok(());
    };
    let result = sqlx::query(include_str!("credentials-expand.sql"))
        .bind(root.try_get::<Uuid, _>("principal_id")?)
        .bind(root.try_get::<Uuid, _>("credential_id")?)
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
        "UPDATE logout_credential_intents SET cursor=COALESCE($2,cursor),expanded=$3 WHERE id=$1",
    )
    .bind(root.try_get::<i64, _>("id")?)
    .bind(result.try_get::<Option<Uuid>, _>("cursor")?)
    .bind(result.try_get::<i64, _>("targets")? < 100)
    .execute(&mut **tx)
    .await?;
    one(changed.rows_affected())
}
pub(super) async fn cleanup(tx: &mut Tx<'_>, cutoff: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM logout_credential_intents WHERE id IN (SELECT id FROM logout_credential_intents WHERE expanded AND created_ms<$1 ORDER BY created_ms,id LIMIT 100 FOR UPDATE SKIP LOCKED)")
        .bind(cutoff).execute(&mut **tx).await?;
    Ok(())
}
