//! Pre-login state consumes no durable capacity; admission follows fresh authentication.
use super::*;

pub(super) async fn continue_login(
    store: &PostgresStore,
    pending: Preauthorization,
    handle: [u8; 32],
    session: Option<[u8; 32]>,
    decision: Decision,
) -> Result<Outcome, Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    let revision = authority::lock(&mut tx).await?;
    let catalog = records::catalog(&mut tx, pending.request.client).await?;
    unchanged(&pending, &catalog.policy, revision)?;
    let current = authority::session(&mut tx, session).await?;
    let now = authority::now(&mut tx).await?;
    let consented = authority::consent(&mut tx, &pending.request, &catalog.policy, current).await?;
    let stage = interaction(
        &pending.request,
        pending.created_ms,
        now,
        pending.initial_session,
        None,
        current,
        consented,
    )?;
    if stage == Interaction::Login {
        if decision != Decision::Inspect {
            return Err(Error::LoginRequired);
        }
        return Ok(view(catalog.name, &pending.request, stage));
    }
    admit(&mut tx, &pending, &catalog.policy, handle, current, stage).await?;
    let outcome = resume(&mut tx, handle, session, decision).await?;
    tx.commit().await.map_err(storage)?;
    Ok(outcome)
}

fn unchanged(
    pending: &Preauthorization,
    policy: &ClientPolicy,
    revision: u64,
) -> Result<(), Error> {
    if pending.client_revision != policy.revision
        || pending.application_revision != policy.application_revision
        || pending.resource_policy_revision != pending.request.resource.as_ref().map(|_| revision)
    {
        return Err(Error::InvalidTransaction);
    }
    authorize_request(&pending.request, policy).map_err(|_| Error::InvalidTransaction)
}

async fn admit(
    tx: &mut Tx<'_>,
    pending: &Preauthorization,
    policy: &ClientPolicy,
    handle: [u8; 32],
    current: Option<Session>,
    stage: Interaction,
) -> Result<(), Error> {
    // Existing records use their own row lock in resume, including terminal records.
    if exists(tx, handle).await? {
        return Ok(());
    }
    // Serialize only first materialization, then recheck after concurrent creators.
    writes::capacity_lock(tx).await?;
    if exists(tx, handle).await? {
        return Ok(());
    }
    writes::capacity(tx, pending.request.client).await?;
    writes::insert(
        tx,
        &pending.request,
        policy,
        handle,
        pending.initial_session,
        pending.created_ms,
    )
    .await?;
    writes::bind(tx, handle, current, stage).await
}
async fn exists(tx: &mut Tx<'_>, handle: [u8; 32]) -> Result<bool, Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM authorization_requests WHERE digest=$1)")
        .bind(handle.as_slice())
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)
}
