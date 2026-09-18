//! Infrastructure-independent identity types and computations.

pub mod authentication;
pub mod authorization;
pub mod directory;
pub mod identity;

/// Account activation state; credentials and verification have separate lifecycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Inactive,
}

pub mod limiter_recovery;
pub mod limiting;
