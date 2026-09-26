use super::output::{Failure, Output};
#[derive(Clone, Copy)]
pub(super) enum Operation {
    Fence,
    Activate,
    Status,
    Inspect(OperationId),
}
use crate::{redis_configuration, redis_limiter::RedisCounters};
use darkhorse_application::limiter_activation::{self, Error, Journal};
use darkhorse_application::shared_limiting::{EnforcementAuthority, RecoveryAuthority};
use darkhorse_domain::{
    identity::OperationId,
    limiter_recovery::{Enforcement, activation_ready},
};
mod journal;

const FAILURE: &str = "Limiter operation failed or enforcement is untrusted; inspect state and follow the recovery procedure.";
struct RecoveryEnvironment;
impl envbind::Environment for RecoveryEnvironment {
    fn get(&self, name: &str) -> Result<Option<String>, envbind::EnvironmentError> {
        crate::deployment_environment::DeploymentEnvironment.get(
            if name == "DARKHORSE_REDIS_LIMITER_URL" {
                "DARKHORSE_REDIS_LIMITER_ADMIN_URL"
            } else {
                name
            },
        )
    }
}
pub(super) async fn run(command: Operation) -> Result<Output, Failure> {
    let store = super::connect(32).await?;
    let result = execute(&store, command).await;
    store.close().await;
    result
}
async fn execute(
    store: &crate::postgres::PostgresStore,
    command: Operation,
) -> Result<Output, Failure> {
    match command {
        Operation::Fence => fence(store).await,
        Operation::Activate => activate(store).await,
        Operation::Status => status(store).await,
        Operation::Inspect(id) => store
            .inspect(id)
            .await
            .map(journal::project)
            .map(Output::record)
            .map_err(|e| journal::failure(e, id)),
    }
}
async fn fence(store: &crate::postgres::PostgresStore) -> Result<Output, Failure> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| FAILURE)?;
    let state = store.fence(nonce).await.map_err(|_| FAILURE)?;
    Ok(Output::record(project(state, None)))
}
async fn activate(store: &crate::postgres::PostgresStore) -> Result<Output, Failure> {
    let id = journal::operation_id()?;
    activation(store, id)
        .await
        .map_err(|error| journal::failure(error, id))?;
    let correlation = uuid::Uuid::from_u128(id.as_u128()).to_string();
    Ok(Output::localized_message(
        format!(
            "Limiter generation activated. Correlation: {correlation}. Use limiter-status to inspect enforcement."
        ),
        format!(
            "Generación del limitador activada. Correlación: {correlation}. Use limiter-status para consultar su aplicación."
        ),
        serde_json::json!({"activated":true,"operation_id":correlation}),
    ))
}
async fn activation(store: &crate::postgres::PostgresStore, id: OperationId) -> Result<(), Error> {
    activation_ready(store.read().await.map_err(|_| Error::Unavailable)?)
        .map_err(|_| Error::NotReady)?;
    let settings =
        redis_configuration::load(RecoveryEnvironment).map_err(|_| Error::Unavailable)?;
    let counters = RedisCounters::new(settings).map_err(|_| Error::Unavailable)?;
    limiter_activation::activate(store, &counters, id).await
}

async fn status(store: &crate::postgres::PostgresStore) -> Result<Output, Failure> {
    let state = store.read().await.map_err(|_| FAILURE)?;
    let count = if state.active {
        let counters = RedisCounters::new(
            redis_configuration::load(crate::deployment_environment::DeploymentEnvironment)
                .map_err(|_| FAILURE)?,
        )
        .map_err(|_| FAILURE)?;
        Some(counters.status(state).await.map_err(|_| FAILURE)?)
    } else {
        None
    };
    Ok(Output::record(project(state, count)))
}

fn project(state: Enforcement, count: Option<u64>) -> serde_json::Value {
    serde_json::json!({"epoch":state.generation.epoch(),"phase":if state.active {"active"}else{"cooling"},"not_before_ms":state.not_before_ms,"database_ms":state.now_ms,"counter_entries":count,"capacity":crate::redis_limiter::CAPACITY})
}
