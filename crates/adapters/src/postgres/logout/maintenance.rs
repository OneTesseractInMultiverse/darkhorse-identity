use super::*;

// One root and at most 100 associations per transaction, even for old sessions
// with arbitrarily many clients. Cursor advance and inserts commit together.
pub(super) async fn expand(tx: &mut Tx<'_>) -> Result<(), sqlx::Error> {
    let root = sqlx::query("SELECT * FROM logout_intents WHERE NOT expanded ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 1")
        .fetch_optional(&mut **tx).await?;
    let Some(root) = root else {
        return Ok(());
    };
    let id: i64 = root.try_get("id")?;
    let result = sqlx::query(
        r#"
        WITH targets AS MATERIALIZED (
            SELECT sid,client_id FROM relying_party_sessions
            WHERE session_id=$1 AND ($2::uuid IS NULL OR client_id>$2)
            ORDER BY client_id LIMIT 100
        ), eligible AS MATERIALIZED (
            SELECT t.*,d.revision FROM targets t JOIN LATERAL (
                SELECT revision,uri FROM logout_destinations
                WHERE client_id=t.client_id AND ordering<$3 ORDER BY ordering DESC LIMIT 1
            ) d ON d.uri IS NOT NULL
        ), inserted AS (
            INSERT INTO logout_deliveries(intent_id,sid,client_id,revision,created_ms,next_ms)
            SELECT $3,sid,client_id,revision,$4,$4 FROM eligible RETURNING id
        )
        SELECT (SELECT count(*) FROM targets) AS targets,
            (SELECT client_id FROM targets ORDER BY client_id DESC LIMIT 1) AS cursor,
            (SELECT count(*) FROM eligible) AS eligible,
            (SELECT count(*) FROM inserted) AS inserted
    "#,
    )
    .bind(root.try_get::<Uuid, _>("session_id")?)
    .bind(root.try_get::<Option<Uuid>, _>("cursor")?)
    .bind(id)
    .bind(root.try_get::<i64, _>("created_ms")?)
    .fetch_one(&mut **tx)
    .await?;
    if result.try_get::<i64, _>("eligible")? != result.try_get::<i64, _>("inserted")? {
        return Err(sqlx::Error::RowNotFound);
    }
    let changed =
        sqlx::query("UPDATE logout_intents SET cursor=COALESCE($2,cursor),expanded=$3 WHERE id=$1")
            .bind(id)
            .bind(result.try_get::<Option<Uuid>, _>("cursor")?)
            .bind(result.try_get::<i64, _>("targets")? < 100)
            .execute(&mut **tx)
            .await?;
    one(changed.rows_affected())?;
    Ok(())
}

pub(super) async fn cleanup(tx: &mut Tx<'_>, now: i64) -> Result<(), sqlx::Error> {
    let cutoff = now.saturating_sub(darkhorse_domain::logout::RETENTION_MS as i64);
    // Keep audit and destination history. Delete only terminal payloads and
    // completely expanded roots without remaining work, in bounded batches.
    sqlx::query("DELETE FROM logout_deliveries WHERE id IN (SELECT id FROM logout_deliveries WHERE state<>'queued' AND created_ms<$1 ORDER BY created_ms,id LIMIT 100 FOR UPDATE SKIP LOCKED)")
        .bind(cutoff).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM logout_intents WHERE id IN (SELECT i.id FROM logout_intents i WHERE i.expanded AND i.created_ms<$1 AND NOT EXISTS(SELECT 1 FROM logout_deliveries d WHERE d.intent_id=i.id) ORDER BY i.created_ms,i.id LIMIT 100 FOR UPDATE SKIP LOCKED)")
        .bind(cutoff).execute(&mut **tx).await?;
    Ok(())
}
