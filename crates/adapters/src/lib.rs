//! Infrastructure adapters for configuration, HTTP, and directory query syntax.

pub mod admin_catalog_http;
pub mod admin_directory_http;
pub mod authentication_configuration;
pub mod authentication_http;
pub mod configuration;
pub mod database_configuration;
pub mod directory_query;
pub mod http;
pub mod json;
pub mod login_admission;
pub mod operator;
pub mod password;
pub mod postgres;
pub mod registration;
pub mod registration_http;
pub mod session_secret;
pub mod signing;

pub mod redis_configuration;

pub mod provider_http;
pub mod redis_infrastructure;
pub mod redis_limiter;
pub mod token_http;
pub mod tokens;

#[cfg(test)]
#[path = "../tests/unit/signing/fixture.rs"]
mod signing_fixture;

pub mod resource_servers;
pub mod resource_servers_http;

pub mod email_verification;
pub mod email_verification_http;
pub mod sessions_http;

pub mod invitations_http;
pub mod personal_keys;
pub mod personal_keys_http;

pub mod profiles;

pub mod profiles_http;

pub mod media;

pub mod media_http;
