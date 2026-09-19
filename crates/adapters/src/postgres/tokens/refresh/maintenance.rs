use super::*;
use darkhorse_application::refresh::RefreshMaintenance;
impl RefreshMaintenance for PostgresStore {
    async fn prune_refresh(&self) -> Result<u64, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        let Some(cutoff) = policy::retention_cutoff(now) else {
            return Ok(0);
        };
        // Lock roots in one order and skip readers/rotators or another worker.
        // Each retained family contains at most 256 members and 256 access rows.
        let count=sqlx::query("WITH expired AS (SELECT c.digest FROM refresh_families f JOIN authorization_codes c ON c.digest=f.code_digest WHERE f.expires_ms<=$1 ORDER BY f.expires_ms,f.code_digest LIMIT $2 FOR UPDATE OF c SKIP LOCKED) DELETE FROM refresh_families f USING expired e WHERE f.code_digest=e.digest")
            .bind(cutoff as i64).bind(i64::from(policy::CLEANUP_BATCH)).execute(&mut *tx).await.map_err(storage)?.rows_affected();
        tx.commit().await.map_err(storage)?;
        Ok(count)
    }
}
