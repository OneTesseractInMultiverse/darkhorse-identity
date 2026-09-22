use super::output::{Failure, Output};
use crate::{
    redis_configuration,
    redis_infrastructure::{Identity, ProbeFailure, RedisInfrastructure},
};

pub(super) async fn run() -> Result<Output, Failure> {
    let settings = redis_configuration::load(crate::deployment_environment::DeploymentEnvironment)
        .map_err(|_| "Invalid Redis configuration; check DARKHORSE_REDIS_* settings.")?;
    let infrastructure =
        RedisInfrastructure::new(settings).map_err(|_| "Cannot prepare Redis connections.")?;
    let status = infrastructure.inspect().await;
    let value = serde_json::json!({"cache":project(&status.cache),"limiter":project(&status.limiter),"shared_enforcement":"not_checked"});
    match result(&status.cache, &status.limiter) {
        Ok(()) => Ok(Output::record(value)),
        Err(message) => Err(Failure::from(message).with_data(value)),
    }
}
fn result(
    cache: &Result<Identity, ProbeFailure>,
    limiter: &Result<Identity, ProbeFailure>,
) -> Result<(), &'static str> {
    if cache.is_err() || limiter.is_err() {
        Err("Redis infrastructure check failed; inspect the per-role result.")
    } else {
        Ok(())
    }
}
fn project(status: &Result<Identity, ProbeFailure>) -> serde_json::Value {
    match status {
        Ok(id) => {
            serde_json::json!({"connection":"reachable","run_id":id.run_id,"memory_limit_bytes":id.memory_limit_bytes,"used_memory_bytes":id.used_memory_bytes})
        }
        Err(error) => {
            serde_json::json!({"connection":match error {ProbeFailure::Unavailable=>"unavailable",ProbeFailure::UnsafeConfiguration=>"unsafe_configuration",ProbeFailure::SharedInstance=>"shared_instance"}})
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/redis_status.rs"]
mod tests;
