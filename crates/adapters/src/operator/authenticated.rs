//! Shared protected input and bounded infrastructure for authenticated commands.
use crate::{
    authentication_configuration,
    deployment_environment::DeploymentEnvironment,
    login_admission::SharedLoginAdmission,
    postgres::PostgresStore,
    redis_configuration::{self, RedisSettings},
    redis_limiter::RedisLimiter,
};
use darkhorse_domain::operator_accounts::Error;
mod input;
pub(super) use input::Input;
pub(super) struct Context {
    pub store: PostgresStore,
    pub admission: SharedLoginAdmission,
}
pub(super) async fn credentials(stdin: bool, mutation: bool) -> Result<Input, &'static str> {
    if stdin {
        input::read(std::io::stdin().lock())
    } else {
        input::interactive(mutation).await
    }
}
pub(super) async fn connect() -> Result<Context, Error> {
    let authentication = authentication_configuration::load(DeploymentEnvironment)
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    let redis = limit_redis(
        redis_configuration::load(DeploymentEnvironment).map_err(|_| Error::Unavailable)?,
    );
    let store = super::connect(2).await.map_err(|_| Error::Unavailable)?;
    let result = admission(&store, authentication, redis).await;
    match result {
        Ok(admission) => Ok(Context { store, admission }),
        Err(error) => {
            store.close().await;
            Err(error)
        }
    }
}
async fn admission(
    store: &PostgresStore,
    authentication: authentication_configuration::AuthenticationSettings,
    redis: RedisSettings,
) -> Result<SharedLoginAdmission, Error> {
    store
        .bind_login_key(authentication.key_digest())
        .await
        .map_err(|_| Error::Unavailable)?;
    let limiter = RedisLimiter::new(store.clone(), redis).map_err(|_| Error::Unavailable)?;
    Ok(SharedLoginAdmission::new(limiter, authentication.key))
}
fn limit_redis(mut settings: RedisSettings) -> RedisSettings {
    settings.limiter.connections = 1;
    settings
}
#[cfg(test)]
#[path = "../../tests/unit/operator/authenticated.rs"]
mod tests;
