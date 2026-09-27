use super::*;
use darkhorse_application::oidc_maintenance::{
    AUTHORIZATION_REQUEST_CLEANUP_BATCH, AuthorizationMaintenance, AuthorizationRequestCleanupBatch,
};

#[cfg(test)]
#[path = "../../../tests/unit/postgres/oidc/maintenance.rs"]
mod tests;

fn cleanup_batch(
    deleted: i64,
    oldest_deleted_age_ms: Option<i64>,
    oldest_remaining_age_ms: Option<i64>,
) -> Result<AuthorizationRequestCleanupBatch, Error> {
    let deleted = u64::try_from(deleted).map_err(|_| Error::Unavailable)?;
    let oldest_deleted_age_ms = oldest_deleted_age_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| Error::Unavailable)?;
    let oldest_remaining_age_ms = oldest_remaining_age_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| Error::Unavailable)?;
    let oldest_expired_age_ms = match (oldest_deleted_age_ms, oldest_remaining_age_ms) {
        (Some(deleted), Some(remaining)) => Some(deleted.max(remaining)),
        (Some(deleted), None) => Some(deleted),
        (None, Some(remaining)) => Some(remaining),
        (None, None) => None,
    };
    Ok(AuthorizationRequestCleanupBatch {
        deleted,
        oldest_expired_age_ms,
        backlog_remaining: oldest_remaining_age_ms.is_some(),
    })
}

impl AuthorizationMaintenance for PostgresStore {
    async fn prune_expired_authorization_requests(
        &self,
    ) -> Result<AuthorizationRequestCleanupBatch, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query("SET LOCAL lock_timeout = '500ms'")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("SET LOCAL statement_timeout = '2s'")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        authority::lock(&mut tx).await?;
        let now = authority::now(&mut tx).await?;
        let now = i64::try_from(now).map_err(|_| Error::Unavailable)?;
        let (deleted, oldest_deleted_age_ms, oldest_remaining_age_ms): (
            i64,
            Option<i64>,
            Option<i64>,
        ) = sqlx::query_as(
            "WITH expired AS MATERIALIZED (SELECT digest FROM authorization_requests WHERE expires_ms<=$1 ORDER BY expires_ms,digest LIMIT $2 FOR UPDATE SKIP LOCKED), removed AS (DELETE FROM authorization_requests r USING expired e WHERE r.digest=e.digest RETURNING r.digest,r.expires_ms), remaining AS (SELECT r.digest,r.expires_ms FROM authorization_requests r WHERE r.expires_ms<=$1 AND NOT EXISTS (SELECT 1 FROM removed d WHERE d.digest=r.digest) ORDER BY r.expires_ms,r.digest LIMIT 1) SELECT (SELECT count(*)::bigint FROM removed),(SELECT max($1-expires_ms)::bigint FROM removed),(SELECT ($1-expires_ms)::bigint FROM remaining)",
        )
            .bind(now)
            .bind(i64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH))
            .fetch_one(&mut *tx)
        .await
            .map_err(storage)?;
        let batch = cleanup_batch(deleted, oldest_deleted_age_ms, oldest_remaining_age_ms)?;
        tx.commit().await.map_err(storage)?;
        Ok(batch)
    }
}
