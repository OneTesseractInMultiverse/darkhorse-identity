//! Atomic persistence contracts; callers establish operator/actor authority first.
use darkhorse_domain::{
    AccountStatus,
    directory::{AccountAction, AccountChange, DirectoryError, Profile},
    identity::PrincipalId,
};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryFailure {
    Unavailable,
    NotFound,
    Conflict,
    Policy(DirectoryError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    pub id: PrincipalId,
    pub profile: Profile,
    pub status: AccountStatus,
    pub credential_epoch: u64,
    pub revision: u64,
    pub administrator: bool,
    /// Active member with a supported, nonrevoked credential record.
    pub eligible_administrator: bool,
}

pub trait DirectoryStore {
    fn account(
        &self,
        id: PrincipalId,
    ) -> impl Future<Output = Result<AccountRecord, DirectoryFailure>> + Send;
    /// Load locked facts, run the domain transition, and persist state and audit
    /// in one transaction. A stale expected revision never overwrites newer state.
    fn change(
        &self,
        id: PrincipalId,
        expected_revision: u64,
        action: AccountAction,
    ) -> impl Future<Output = Result<Option<AccountChange>, DirectoryFailure>> + Send;
}
