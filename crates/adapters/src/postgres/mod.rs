use crate::database_configuration::DatabaseSettings;
use darkhorse_application::{
    bootstrap::{BootstrapError, BootstrapStore, NewAdministrator},
    directory::{AccountRecord, DirectoryFailure, DirectoryStore},
};
use darkhorse_domain::{
    directory::{AccountAction, AccountChange},
    identity::PrincipalId,
};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{str::FromStr, time::Duration};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
mod admin_catalog;
mod admin_directory;
mod authentication;
mod bootstrap;
mod directory;
mod limiting;
mod personal_keys;
mod registration;
mod sessions;
mod signing;

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn connect(settings: DatabaseSettings) -> Result<Self, DirectoryFailure> {
        let options = connection_options(&settings)?;
        let pool = PgPoolOptions::new()
            .max_connections(settings.max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(|_| DirectoryFailure::Unavailable)?;
        Ok(Self::from_pool(pool))
    }
    pub async fn migrate(&self) -> Result<(), DirectoryFailure> {
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|_| DirectoryFailure::Unavailable)
    }
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

// SQLx's constructor reads PostgreSQL environment/TLS defaults. Keep it at the
// effect boundary, outside configuration computations and service-free tests.
fn connection_options(settings: &DatabaseSettings) -> Result<PgConnectOptions, DirectoryFailure> {
    Ok(PgConnectOptions::from_str(settings.url.as_str())
        .map_err(|_| DirectoryFailure::Unavailable)?
        .port(settings.url.port().unwrap_or(5432))
        .ssl_mode(if settings.insecure {
            PgSslMode::Disable
        } else {
            PgSslMode::VerifyFull
        })
        .application_name("darkhorse")
        .options([
            ("search_path", "public"),
            ("statement_timeout", "5000"),
            ("lock_timeout", "3000"),
            ("idle_in_transaction_session_timeout", "10000"),
        ])
        .disable_statement_logging())
}

impl BootstrapStore for PostgresStore {
    async fn bootstrap(
        &self,
        administrator: NewAdministrator,
    ) -> Result<PrincipalId, BootstrapError> {
        bootstrap::bootstrap(&self.pool, administrator).await
    }
}
impl DirectoryStore for PostgresStore {
    async fn account(&self, id: PrincipalId) -> Result<AccountRecord, DirectoryFailure> {
        directory::account(&self.pool, id).await
    }
    async fn change(
        &self,
        id: PrincipalId,
        expected_revision: u64,
        action: AccountAction,
    ) -> Result<Option<AccountChange>, DirectoryFailure> {
        directory::change(&self.pool, id, expected_revision, action).await
    }
}
mod oidc;
mod tokens;

mod resource_authority;

mod resource_servers;

pub mod profiling;

mod email_verification;

mod email_queue;
mod invitations;

mod profiles;

mod media;
