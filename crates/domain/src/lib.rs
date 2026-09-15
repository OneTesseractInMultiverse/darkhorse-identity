//! Infrastructure-independent identity types and computations.

/// Account activation state; credentials and verification have separate lifecycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Inactive,
}
