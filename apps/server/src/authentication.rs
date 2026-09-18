use darkhorse_adapters::{
    authentication_configuration, authentication_http, database_configuration,
    login_admission::SharedLoginAdmission, password::PasswordPreparation, postgres::PostgresStore,
    redis_configuration, redis_limiter::RedisLimiter, session_secret::OsSessionEntropy,
};
use darkhorse_application::authentication::Service;

pub async fn router(
    settings: &darkhorse_adapters::configuration::HttpSettings,
) -> Result<axum::Router, &'static str> {
    let authentication = authentication_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid login configuration.")?;
    let Some(authentication) = authentication else {
        return Ok(authentication_http::disabled_router());
    };
    let database = database_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid database configuration.")?;
    let redis = redis_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid Redis configuration.")?;
    let store = PostgresStore::connect(database)
        .await
        .map_err(|_| "Authentication database unavailable.")?;
    store
        .bind_login_key(authentication.key_digest())
        .await
        .map_err(
            |_| "Login limiter key does not match the deployment or storage is unavailable.",
        )?;
    let limiter =
        RedisLimiter::new(store.clone(), redis).map_err(|_| "Cannot initialize login limiter.")?;
    let service = Service {
        store,
        admission: SharedLoginAdmission::new(limiter, authentication.key),
        passwords: PasswordPreparation::default(),
        entropy: OsSessionEntropy,
    };
    Ok(authentication_http::router(
        service,
        settings.public_origin.clone(),
    ))
}
