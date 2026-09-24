//! Infrastructure-independent identity types and computations.

pub mod admin_catalog;
pub mod admin_directory;
pub mod authentication;
pub mod authorization;
pub mod directory;
pub mod identity;
pub mod registration;
pub mod signing;

/// Account activation state; credentials and verification have separate lifecycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Inactive,
}

pub mod limiter_recovery;
pub mod limiting;

pub mod oidc;
pub mod refresh;
pub mod tokens;

pub mod email_verification;
pub mod sessions;

pub mod email_delivery;
pub mod invitations;
pub mod personal_keys;

pub mod profiles;

pub mod media;
pub mod operator_accounts;
pub mod operator_directory;

pub mod operator_applications;
pub mod operator_catalog;
