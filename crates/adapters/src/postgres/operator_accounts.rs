use super::{PostgresStore, authentication, directory, sessions};
use darkhorse_application::{
    authentication::AuthError,
    directory::DirectoryFailure,
    operator_accounts::{CandidateAt, Outcome, Store, Verified},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Authority, Error, Operation, Request, authorize},
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
mod audit;
type Tx<'a> = Transaction<'a, Postgres>;
impl Store for PostgresStore {
    type Request = Request;
    type Outcome = Outcome;
    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        authentication::candidate_record(&self.pool, email)
            .await
            .map_err(storage)?
            .as_ref()
            .map(candidate)
            .transpose()
    }
    async fn denied(&self, id: OperationId, request: &Request) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        audit::insert(&mut tx, id, request, None, &Err(Error::Denied)).await?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }
    async fn execute(&self, proof: Verified) -> Result<Outcome, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Operator reads also use the exclusive security fence so actor checks,
        // target locks, mutation and audit form one ordered transaction.
        sessions::lock(&mut tx, true).await.map_err(storage)?;
        let result = match authority(&mut tx, &proof).await {
            Ok(()) => execute(&mut tx, proof.request().operation()).await,
            Err(error) => Err(error),
        };
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        audit::insert(
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
fn candidate(row: &sqlx::postgres::PgRow) -> Result<CandidateAt, Error> {
    Ok(CandidateAt {
        credential: authentication::candidate(row).map_err(storage)?,
        observed_ms: row
            .try_get::<i64, _>("observed_ms")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
    })
}
pub(super) async fn authority<R>(tx: &mut Tx<'_>, proof: &Verified<R>) -> Result<(), Error> {
    let candidate = proof.candidate();
    let current = match authentication::recheck(tx, &candidate.credential).await {
        Ok(_) => true,
        Err(AuthError::Denied) => false,
        Err(_) => return Err(Error::Unavailable),
    };
    let administrator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM platform_administrators WHERE principal_id=$1)",
    )
    .bind(Uuid::from_u128(candidate.credential.principal.as_u128()))
    .fetch_one(&mut **tx)
    .await
    .map_err(storage)?;
    let now = sessions::now(tx).await.map_err(storage)?;
    authorize(
        Authority {
            credential_current: current,
            administrator,
            observed_ms: candidate.observed_ms,
        },
        now,
    )
}
async fn execute(tx: &mut Tx<'_>, operation: Operation) -> Result<Outcome, Error> {
    match operation {
        Operation::Show(target) => Ok(Outcome {
            account: directory::locked_account(tx, target)
                .await
                .map_err(directory_error)?,
            changed: false,
        }),
        Operation::Change {
            target,
            revision,
            action,
        } => {
            let change = directory::change_locked(tx, target, revision, action)
                .await
                .map_err(directory_error)?;
            let account = directory::locked_account(tx, target)
                .await
                .map_err(directory_error)?;
            Ok(Outcome {
                account,
                changed: change.is_some(),
            })
        }
    }
}
fn directory_error(error: DirectoryFailure) -> Error {
    match error {
        DirectoryFailure::NotFound => Error::NotFound,
        DirectoryFailure::Conflict => Error::Conflict,
        DirectoryFailure::Policy(_) => Error::PolicyRejected,
        DirectoryFailure::Unavailable => Error::Unavailable,
    }
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
