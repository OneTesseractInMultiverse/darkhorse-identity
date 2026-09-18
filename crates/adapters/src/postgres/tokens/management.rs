use super::*;
impl TokenManagementStore for PostgresStore {
    async fn introspect(
        &self,
        input: Management,
        issuer: &str,
    ) -> Result<Option<ActiveToken>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        reads::client(&mut tx, input.client, input.secret).await?;
        let active = inspect(&mut tx, input, issuer).await?;
        tx.commit().await.map_err(storage)?;
        Ok(active)
    }
    async fn revoke(&self, input: Management, issuer: &str) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        reads::client(&mut tx, input.client, input.secret).await?;
        revoke(&mut tx, input, issuer).await?;
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
}
async fn inspect(
    tx: &mut Tx<'_>,
    input: Management,
    issuer: &str,
) -> Result<Option<ActiveToken>, Error> {
    let Some(token) = input.token else {
        return Ok(None);
    };
    match access::load(tx, token, issuer, Some(input.client)).await {
        Ok(stored) => Ok(Some(access::metadata(stored))),
        Err(Error::InvalidToken) => Ok(None),
        Err(error) => Err(error),
    }
}
async fn revoke(tx: &mut Tx<'_>, input: Management, issuer: &str) -> Result<(), Error> {
    let Some(token) = input.token else {
        return Ok(());
    };
    let code_digest = match access::lookup(tx, token, issuer, Some(input.client)).await {
        Ok(code) => code,
        Err(Error::InvalidToken) => return Ok(()),
        Err(error) => return Err(error),
    };
    let code = reads::code(tx, code_digest).await?;
    let affected =
        sqlx::query("UPDATE access_tokens SET revoked=true WHERE digest=$1 AND NOT revoked")
            .bind(token.as_slice())
            .execute(&mut **tx)
            .await
            .map_err(storage)?
            .rows_affected();
    if affected > 0 {
        let now = authority::now(tx).await.map_err(storage)?;
        audit(tx, code.principal, code.client, "access_revoked", now).await?;
    }
    Ok(())
}
