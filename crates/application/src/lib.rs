//! Project-owned use-case contracts. Transport and database types stay outside.

use darkhorse_domain::AccountStatus;

pub mod admin_catalog;
pub mod admin_directory;
pub mod authentication;
pub mod bootstrap;
pub mod directory;
pub mod registration;
pub mod signing;

/// Bounded directory criteria. A future use case adds mandatory actor restrictions.
/// This value neither authorizes access nor describes database identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryCriteria {
    pub status: Option<AccountStatus>,
    pub limit: u16,
    pub offset: u16,
}

pub mod limiting;
pub mod oidc;
pub mod refresh;
pub mod resource_servers;
pub mod shared_limiting;
pub mod tokens;

pub mod email_verification;
pub mod sessions;

pub mod credentials;
pub mod email_delivery;
pub mod invitations;
pub mod personal_keys;

pub mod profiles;

pub mod media;
pub mod operator_accounts;
