#[derive(Clone, Copy)]
pub(super) enum Operation {
    Fence,
    Activate,
    Status,
}
use crate::{redis_configuration, redis_limiter::RedisCounters};
use darkhorse_application::shared_limiting::{EnforcementAuthority, RecoveryAuthority};
use darkhorse_domain::limiter_recovery::{Enforcement, activation_ready};

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
pub(super) async fn run(command: Operation) -> Result<(), &'static str> {
    let store = super::connect().await?;
    let result = execute(&store, command).await;
    store.close().await;
    result
}
async fn execute(
    store: &crate::postgres::PostgresStore,
    command: Operation,
) -> Result<(), &'static str> {
    match command {
        Operation::Fence => fence(store).await,
        Operation::Activate => activate(store).await,
        Operation::Status => status(store).await,
    }
}
async fn fence(store: &crate::postgres::PostgresStore) -> Result<(), &'static str> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| FAILURE)?;
    let state = store.fence(nonce).await.map_err(|_| FAILURE)?;
    println!("{}", project(state, None));
    Ok(())
}
async fn activate(store: &crate::postgres::PostgresStore) -> Result<(), &'static str> {
    let state = store.read().await.map_err(|_| FAILURE)?;
    activation_ready(state).map_err(
        |_| "Limiter recovery wait has not elapsed, or the generation is already active.",
    )?;
    let settings = redis_configuration::load(RecoveryEnvironment)
        .map_err(|_| "Invalid operator Redis configuration.")?;
    let counters = RedisCounters::new(settings).map_err(|_| FAILURE)?;
    let identity = counters.initialize(state).await.map_err(|_| FAILURE)?;
    store
        .activate(state.generation, identity)
        .await
        .map_err(|_| FAILURE)?;
    println!("Limiter generation activated. Use limiter-status to inspect enforcement.");
    Ok(())
}
async fn status(store: &crate::postgres::PostgresStore) -> Result<(), &'static str> {
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
    println!("{}", project(state, count));
    Ok(())
}

fn project(state: Enforcement, count: Option<u64>) -> serde_json::Value {
    serde_json::json!({"epoch":state.generation.epoch(),"phase":if state.active {"active"}else{"cooling"},"not_before_ms":state.not_before_ms,"database_ms":state.now_ms,"counter_entries":count,"capacity":crate::redis_limiter::CAPACITY})
}
