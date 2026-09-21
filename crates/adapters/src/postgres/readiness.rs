use super::PostgresStore;
impl crate::readiness::Readiness for PostgresStore {
    async fn ready(&self) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT NOT pg_is_in_recovery()")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(false)
    }
}
