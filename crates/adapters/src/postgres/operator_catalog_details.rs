use super::{PostgresStore, operator_accounts, registration, sessions};
use darkhorse_application::{
    operator_accounts::{CandidateAt, Store, Verified},
    registration::{ReadTarget, Record},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, needs_current_authority},
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub struct CatalogDetails<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_catalog_details(&self) -> CatalogDetails<'_> {
        CatalogDetails(self)
    }
}
impl Store for CatalogDetails<'_> {
    type Request = ReadTarget;
    type Outcome = Record;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }
    async fn denied(&self, id: OperationId, target: &ReadTarget) -> Result<(), Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        audit(&mut tx, id, *target, None, &Err(Error::Denied)).await?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn execute(&self, proof: Verified<ReadTarget>) -> Result<Record, Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        sessions::lock(&mut tx, false).await.map_err(storage)?;
        let result = read(&mut tx, &proof).await;
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        audit(
            &mut tx,
            proof.id(),
            *proof.request(),
            Some(proof.candidate()),
            &result,
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
    proof: &Verified<ReadTarget>,
) -> Result<Record, Error> {
    operator_accounts::authority(tx, proof).await?;
    let result = registration::configuration_current(tx, *proof.request())
        .await
        .map_err(|error| match error {
            darkhorse_domain::registration::RegistrationError::NotFound => Error::NotFound,
            _ => Error::Unavailable,
        });
    operator_accounts::authority(tx, proof).await?;
    result
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: OperationId,
    target: ReadTarget,
    actor: Option<&CandidateAt>,
    result: &Result<Record, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let (command, application, client) = match target {
        ReadTarget::Application(app) => ("application.show", app, None),
        ReadTarget::Client {
            application,
            client,
        } => ("client.show", application, Some(client)),
    };
    let inserted=sqlx::query("INSERT INTO operator_catalog_detail_audit(operation_id,command,application_id,client_id,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(Uuid::from_u128(id.as_u128())).bind(command).bind(Uuid::from_u128(application.as_u128()))
        .bind(client.map(|c|Uuid::from_u128(c.as_u128())))
        .bind(actor.map(|a|Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a|Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a|i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a|i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(match result {Ok(_)=>"read",Err(Error::NotFound)=>"not_found",Err(_)=>"denied"})
        .bind(i64::try_from(now).map_err(storage)?).execute(&mut **tx).await.map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
