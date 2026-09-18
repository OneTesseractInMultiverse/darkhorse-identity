use darkhorse_adapters::{
    authentication_configuration, authentication_http, database_configuration,
    login_admission::SharedLoginAdmission, password::PasswordPreparation, postgres::PostgresStore,
    redis_configuration, redis_limiter::RedisLimiter, registration::OsRegistrationEntropy,
    registration_http, session_secret::OsSessionEntropy,
};
use darkhorse_adapters::{provider_http, signing::configuration as provider_configuration};
use darkhorse_application::{authentication::Service, signing::SigningStore};

pub async fn router(
    settings: &darkhorse_adapters::configuration::HttpSettings,
) -> Result<axum::Router, &'static str> {
    let authentication = authentication_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid login configuration.")?;
    let provider = provider_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid provider configuration.")?;
    let Some(authentication) = authentication else {
        if provider.is_some() {
            return Err("Provider requires enabled password authentication.");
        }
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
        store: store.clone(),
        admission: SharedLoginAdmission::new(limiter, authentication.key),
        passwords: PasswordPreparation::default(),
        entropy: OsSessionEntropy,
    };
    let provider = if let Some(wrap) = provider {
        store
            .bind_provider(
                &settings.public_origin.origin().ascii_serialization(),
                wrap.fingerprint(),
            )
            .await
            .map_err(|_| "Provider issuer or wrapping key conflicts with persisted state.")?;
        provider_http::router(store.clone(), settings.public_origin.clone()).merge(
            darkhorse_adapters::token_http::router(
                store.clone(),
                darkhorse_adapters::tokens::signer::Signer::new(wrap),
                settings.public_origin.clone(),
            ),
        )
    } else {
        axum::Router::new()
    };
    let registration = darkhorse_application::registration::Service {
        store,
        entropy: OsRegistrationEntropy,
    };
    Ok(
        authentication_http::router(service, settings.public_origin.clone())
            .merge(provider)
            .merge(registration_http::router(
                registration,
                settings.public_origin.clone(),
            )),
    )
}
