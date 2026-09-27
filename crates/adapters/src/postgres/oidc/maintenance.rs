use super::*;
use darkhorse_application::oidc_maintenance::{
    AUTHORIZATION_REQUEST_CLEANUP_BATCH, AuthorizationMaintenance,
};

impl AuthorizationMaintenance for PostgresStore {
    async fn prune_expired_authorization_requests(&self) -> Result<u64, Error> {
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
        let count=sqlx::query("WITH expired AS (SELECT digest FROM authorization_requests WHERE expires_ms<=$1 ORDER BY expires_ms,digest LIMIT $2 FOR UPDATE SKIP LOCKED) DELETE FROM authorization_requests r USING expired e WHERE r.digest=e.digest")
            .bind(i64::try_from(now).map_err(|_| Error::Unavailable)?)
            .bind(i64::from(AUTHORIZATION_REQUEST_CLEANUP_BATCH))
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        tx.commit().await.map_err(storage)?;
        Ok(count)
    }
}
