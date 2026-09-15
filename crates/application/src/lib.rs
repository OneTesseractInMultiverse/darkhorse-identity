//! Project-owned use-case contracts. Transport and database types stay outside.

use darkhorse_domain::AccountStatus;

/// Bounded directory criteria. A future use case adds mandatory actor restrictions.
/// This value neither authorizes access nor describes database identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryCriteria {
    pub status: Option<AccountStatus>,
    pub limit: u16,
    pub offset: u16,
}
