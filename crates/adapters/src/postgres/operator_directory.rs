use super::{PostgresStore, admin_directory, operator_accounts, sessions};
use darkhorse_application::{
    admin_directory::Page,
    operator_accounts::{CandidateAt, Store, Verified},
};
use darkhorse_domain::{
    AccountStatus,
    identity::OperationId,
    operator_accounts::{Error, needs_current_authority},
    operator_directory::Request,
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub struct DirectoryReader<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_directory(&self) -> DirectoryReader<'_> {
        DirectoryReader(self)
    }
}
impl Store for DirectoryReader<'_> {
    type Request = Request;
    type Outcome = Page;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        audit(&mut tx, id, request, None, None).await?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn execute(&self, proof: Verified<Request>) -> Result<Page, Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        sessions::lock(&mut tx, false).await.map_err(storage)?;
        let result = read(&mut tx, &proof).await;
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        audit(
            &mut tx,
            proof.id(),
            proof.request(),
            Some(proof.candidate()),
            result.as_ref().ok(),
        )
        .await?;
        if needs_current_authority(&result) {
            operator_accounts::authority(&mut tx, &proof).await?;
        }
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        result
    }
}
async fn read(
    tx: &mut Transaction<'_, Postgres>,
    proof: &Verified<Request>,
) -> Result<Page, Error> {
    operator_accounts::authority(tx, proof).await?;
    let page = admin_directory::list_current(
        tx,
        proof.candidate().credential.principal,
        proof.request().query(),
    )
    .await
    .map_err(storage)?;
    operator_accounts::authority(tx, proof).await?;
    Ok(page)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    page: Option<&Page>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let query = request.query();
    let inserted = sqlx::query("INSERT INTO operator_directory_audit(operation_id,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,query_limit,active_filter,after_id,searched,result,returned_count,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(Uuid::from_u128(id.as_u128()))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(i32::from(query.limit))
        .bind(query.status.map(|s| s == AccountStatus::Active))
        .bind(query.after.map(|id| Uuid::from_u128(id.as_u128())))
        .bind(!query.search.is_empty())
        .bind(if page.is_some() { "read" } else { "denied" })
        .bind(page.map(|p| i32::try_from(p.items.len())).transpose().map_err(storage)?)
        .bind(i64::try_from(now).map_err(storage)?)
        .execute(&mut **tx).await.map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
