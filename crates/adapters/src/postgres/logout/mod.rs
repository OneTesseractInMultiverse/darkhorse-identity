use super::PostgresStore;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
pub(super) mod accounts;
mod maintenance;
mod queue;
mod registration;

pub(super) async fn enqueue(tx: &mut Tx<'_>, session: Uuid, now: u64) -> Result<(), sqlx::Error> {
    let target = sqlx::query("SELECT a.id AS ordering,COALESCE(a.created_ms,$2) AS created_ms FROM browser_sessions s JOIN principals p ON p.id=s.principal_id LEFT JOIN logout_account_intents a ON a.principal_id=s.principal_id AND a.credential_epoch=s.credential_epoch WHERE s.public_id=$1 AND (s.credential_epoch=p.credential_epoch OR a.id IS NOT NULL) AND EXISTS(SELECT 1 FROM relying_party_sessions WHERE session_id=$1) AND NOT EXISTS(SELECT 1 FROM logout_intents WHERE session_id=$1)")
    .bind(session)
    .bind(now as i64)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(target) = target {
        let changed = sqlx::query(
            "INSERT INTO logout_intents(session_id,created_ms,destination_order) VALUES($1,$2,$3)",
        )
        .bind(session)
        .bind(target.try_get::<i64, _>("created_ms")?)
        .bind(target.try_get::<Option<i64>, _>("ordering")?)
        .execute(&mut **tx)
        .await?;
        one(changed.rows_affected())?;
    }
    Ok(())
}
async fn audit(
    tx: &mut Tx<'_>,
    client: Uuid,
    delivery: Option<Uuid>,
    revision: i64,
    event: &str,
    now: i64,
    actor: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let changed = sqlx::query("INSERT INTO logout_audit(client_id,delivery_id,revision,event,occurred_ms,actor_id) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(client).bind(delivery).bind(revision).bind(event).bind(now).bind(actor).execute(&mut **tx).await?;
    one(changed.rows_affected())
}

fn one(changed: u64) -> Result<(), sqlx::Error> {
    if changed == 1 {
        Ok(())
    } else {
        Err(sqlx::Error::RowNotFound)
    }
}
