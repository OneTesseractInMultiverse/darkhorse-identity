use super::{PostgresStore, authentication, directory, sessions};
use darkhorse_application::{
    authentication::AuthError,
    directory::DirectoryFailure,
    operator_accounts::{CandidateAt, Outcome, Store, Verified},
};
use darkhorse_domain::{
    AccountStatus,
    identity::OperationId,
    operator_accounts::{
        ActorState, Authority, Error, Operation, Request, authorize, completion_state,
        needs_current_authority,
    },
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
        // Hold the fence through the final authority check, audit and commit.
        // Read-only requests can overlap readers; mutations still exclude them.
        sessions::lock(
            &mut tx,
            matches!(proof.request().operation(), Operation::Change { .. }),
        )
        .await
        .map_err(storage)?;
        let result = match authority(&mut tx, &proof).await {
            Ok(()) => execute(&mut tx, &proof).await,
            Err(error) => Err(error),
        };
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        completion(&mut tx, &proof, &result).await?;
        audit::insert(
            &mut tx,
            proof.id(),
            proof.request(),
            Some(proof.candidate()),
            &result,
        )
        .await?;
        completion(&mut tx, &proof, &result).await?;
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
    authority_state(
        tx,
        proof,
        ActorState {
            status: AccountStatus::Active,
            epoch: proof.candidate().credential.epoch,
        },
    )
    .await
}
async fn authority_state<R>(
    tx: &mut Tx<'_>,
    proof: &Verified<R>,
    state: ActorState,
) -> Result<(), Error> {
    let candidate = proof.candidate();
    let current = match authentication::recheck_state(
        tx,
        &candidate.credential,
        state.status == AccountStatus::Active,
        state.epoch,
    )
    .await
    {
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
async fn completion(
    tx: &mut Tx<'_>,
    proof: &Verified,
    result: &Result<Outcome, Error>,
) -> Result<(), Error> {
    if !needs_current_authority(result) {
        return Ok(());
    }
    let candidate = &proof.candidate().credential;
    let state = completion_state(
        candidate.principal,
        candidate.epoch,
        proof.request().operation(),
        result.as_ref().is_ok_and(|outcome| outcome.changed),
    )?;
    authority_state(tx, proof, state).await
}
async fn execute(tx: &mut Tx<'_>, proof: &Verified) -> Result<Outcome, Error> {
    match proof.request().operation() {
        Operation::Show(target) => Ok(Outcome {
            account: directory::current_account(tx, target)
                .await
                .map_err(directory_error)?,
            changed: false,
        }),
        Operation::Change {
            target,
            revision,
            action,
        } => {
            let change = directory::prepare_change(tx, target, revision, action)
                .await
                .map_err(directory_error)?;
            authority(tx, proof).await?;
            if let Some(change) = change {
                directory::persist(tx, target, change, action)
                    .await
                    .map_err(storage)?;
            }
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
