use darkhorse_adapters::{
    authentication_configuration, authentication_http, database_configuration,
    login_admission::SharedLoginAdmission, password::PasswordPreparation, postgres::PostgresStore,
    redis_configuration, redis_limiter::RedisLimiter, registration::OsRegistrationEntropy,
    registration_http, session_secret::OsSessionEntropy,
};
use darkhorse_adapters::{provider_http, signing::configuration as provider_configuration};
use darkhorse_application::{authentication::Service, signing::SigningStore};

pub struct Runtime {
    pub router: axum::Router,
    pub maintenance: Option<PostgresStore>,
    pub email: Option<(
        PostgresStore,
        darkhorse_adapters::email_verification::smtp::Smtp,
    )>,
}

pub async fn runtime(
    settings: &darkhorse_adapters::configuration::HttpSettings,
) -> Result<Runtime, &'static str> {
    let authentication = authentication_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid login configuration.")?;
    let provider = provider_configuration::load(envbind::ProcessEnvironment)
        .map_err(|_| "Invalid provider configuration.")?;
    let email =
        darkhorse_adapters::email_verification::configuration::load(envbind::ProcessEnvironment)
            .map_err(|_| "Invalid email delivery configuration.")?;
    let Some(authentication) = authentication else {
        if provider.is_some() || email.is_some() {
            return Err("Provider and email verification require enabled password authentication.");
        }
        return Ok(Runtime {
            router: authentication_http::disabled_router(),
            maintenance: None,
            email: None,
        });
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
    let passwords = PasswordPreparation::default();
    let service = Service {
        store: store.clone(),
        admission: SharedLoginAdmission::new(limiter, authentication.key),
        passwords: passwords.clone(),
        entropy: OsSessionEntropy,
    };
    let maintenance = provider.as_ref().map(|_| store.clone());
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
    let resource_registration = darkhorse_application::resource_servers::Service {
        store: store.clone(),
        entropy: darkhorse_adapters::resource_servers::OsResourceEntropy,
    };
    let session_management =
        darkhorse_adapters::sessions_http::router(store.clone(), settings.public_origin.clone());
    let (email_router, email) = email_runtime(email, &store, settings, passwords).await?;
    let registration = darkhorse_application::registration::Service {
        store,
        entropy: OsRegistrationEntropy,
    };
    Ok(Runtime {
        maintenance,
        email,
        router: authentication_http::router(service, settings.public_origin.clone())
            .merge(provider)
            .merge(session_management)
            .merge(email_router)
            .merge(darkhorse_adapters::resource_servers_http::router(
                resource_registration,
                settings.public_origin.clone(),
            ))
            .merge(registration_http::router(
                registration,
                settings.public_origin.clone(),
            )),
    })
}

async fn email_runtime(
    email: Option<darkhorse_adapters::email_verification::configuration::Settings>,
    store: &PostgresStore,
    settings: &darkhorse_adapters::configuration::HttpSettings,
    passwords: PasswordPreparation,
) -> Result<
    (
        axum::Router,
        Option<(
            PostgresStore,
            darkhorse_adapters::email_verification::smtp::Smtp,
        )>,
    ),
    &'static str,
> {
    let Some(email) = email else {
        return Ok((axum::Router::new(), None));
    };
    let origin = settings.public_origin.origin().ascii_serialization();
    let secrets = email.secrets.clone();
    let sender = darkhorse_adapters::email_verification::smtp::Smtp::prepare(email, origin.clone())
        .map_err(|_| "Cannot initialize TLS email delivery.")?;
    store
        .bind_email_delivery(&origin, secrets.fingerprint())
        .await
        .map_err(
            |_| "Email origin or key conflicts with persisted state, or storage is unavailable.",
        )?;
    let router = darkhorse_adapters::email_verification_http::router(
        store.clone(),
        secrets.clone(),
        settings.public_origin.clone(),
    )
    .merge(darkhorse_adapters::invitations_http::router(
        store.clone(),
        secrets,
        passwords,
        settings.public_origin.clone(),
    ));
    Ok((router, Some((store.clone(), sender))))
}
