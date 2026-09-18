//! Project-owned use-case contracts. Transport and database types stay outside.

use darkhorse_domain::AccountStatus;

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
pub mod shared_limiting;
