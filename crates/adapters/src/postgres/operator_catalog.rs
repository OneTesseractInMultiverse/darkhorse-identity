use super::{PostgresStore, admin_catalog, operator_accounts, sessions};
use darkhorse_application::{
    admin_catalog::Page,
    operator_accounts::{CandidateAt, Store, Verified},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::Error,
    operator_catalog::{Request, Target},
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub struct CatalogReader<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_catalog(&self) -> CatalogReader<'_> {
        CatalogReader(self)
    }
}
impl Store for CatalogReader<'_> {
    type Request = Request;
    type Outcome = Page;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        audit(&mut tx, id, request, None, &Err(Error::Denied)).await?;
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
            &result,
        )
        .await?;
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        result
    }
}
async fn read(
    tx: &mut Transaction<'_, Postgres>,
    proof: &Verified<Request>,
) -> Result<Page, Error> {
    operator_accounts::authority(tx, proof).await?;
    let page = admin_catalog::list_current(
        tx,
        match proof.request().target() {
            Target::Applications => darkhorse_application::admin_catalog::List::Applications,
            Target::Clients(id) => darkhorse_application::admin_catalog::List::Clients(id),
        },
        proof.request().query(),
    )
    .await
    .map_err(|e| match e {
        darkhorse_domain::registration::RegistrationError::NotFound => Error::NotFound,
        _ => Error::Unavailable,
    });
    operator_accounts::authority(tx, proof).await?;
    page
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: OperationId,
    request: &Request,
    actor: Option<&CandidateAt>,
    result: &Result<Page, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let query = request.query();
    let (command, application) = match request.target() {
        Target::Applications => ("application.list", None),
        Target::Clients(id) => ("client.list", Some(Uuid::from_u128(id.as_u128()))),
    };
    let inserted = sqlx::query("INSERT INTO operator_catalog_audit(operation_id,command,application_id,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,query_limit,active_filter,after_id,searched,result,returned_count,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
        .bind(Uuid::from_u128(id.as_u128())).bind(command).bind(application)
        .bind(actor.map(|a| Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(i32::from(query.limit)).bind(query.active)
        .bind(query.after.map(|id| Uuid::from_u128(id.get())))
        .bind(!query.search.is_empty())
        .bind(match result { Ok(_) => "read", Err(Error::NotFound) => "not_found", Err(_) => "denied" })
        .bind(result.as_ref().ok().map(|p| i32::try_from(p.items.len())).transpose().map_err(storage)?)
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
