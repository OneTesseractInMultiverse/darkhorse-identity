use super::PostgresStore;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
mod maintenance;
mod queue;
mod registration;

pub(super) async fn enqueue(tx: &mut Tx<'_>, session: Uuid, now: u64) -> Result<(), sqlx::Error> {
    let eligible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM relying_party_sessions WHERE session_id=$1)",
    )
    .bind(session)
    .fetch_one(&mut **tx)
    .await?;
    if eligible {
        let changed =
            sqlx::query("INSERT INTO logout_intents(session_id,created_ms) VALUES($1,$2)")
                .bind(session)
                .bind(now as i64)
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
