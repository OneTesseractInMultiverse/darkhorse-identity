//! Shared transaction boundary for authenticated catalog mutations.
use super::{PostgresStore, operator_accounts, sessions};
use darkhorse_application::operator_accounts::{CandidateAt, Verified};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, needs_current_authority},
};
use sqlx::{Acquire, Postgres, Transaction};
use std::future::Future;
pub(super) type Tx<'a> = Transaction<'a, Postgres>;

pub(super) trait Mutation<R>: Sync {
    type Outcome: Send + Sync;
    fn mutate(
        &self,
        tx: &mut Tx<'_>,
        proof: &Verified<R>,
    ) -> impl Future<Output = Result<Self::Outcome, Error>> + Send;
    fn audit(
        &self,
        tx: &mut Tx<'_>,
        id: OperationId,
        request: &R,
        actor: Option<&CandidateAt>,
        outcome: &Result<Self::Outcome, Error>,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub(super) async fn denied<R: Send + Sync>(
    store: &PostgresStore,
    id: OperationId,
    request: &R,
    mutation: &impl Mutation<R>,
) -> Result<(), Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    mutation
        .audit(&mut tx, id, request, None, &Err(Error::Denied))
        .await?;
    tx.commit().await.map_err(|_| Error::Uncertain)
}
pub(super) async fn execute<R: Send + Sync, M: Mutation<R>>(
    store: &PostgresStore,
    proof: Verified<R>,
    mutation: &M,
) -> Result<M::Outcome, Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    sessions::lock(&mut tx, true).await.map_err(storage)?;
    let result = change(&mut tx, &proof, mutation).await;
    if matches!(result, Err(Error::Unavailable)) {
        return result;
    }
    mutation
        .audit(
            &mut tx,
            proof.id(),
            proof.request(),
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
async fn change<R: Send + Sync, M: Mutation<R>>(
    tx: &mut Tx<'_>,
    proof: &Verified<R>,
    mutation: &M,
) -> Result<M::Outcome, Error> {
    operator_accounts::authority(tx, proof).await?;
    let mut savepoint = tx.begin().await.map_err(storage)?;
    let result = mutation.mutate(&mut savepoint, proof).await;
    if result.is_ok() {
        savepoint.commit().await.map_err(storage)?;
    } else {
        savepoint.rollback().await.map_err(storage)?;
    }
    result
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
