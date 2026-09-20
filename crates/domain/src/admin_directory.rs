//! Bounded directory inputs and profile changes, independent of transport/storage.
use crate::{
    AccountStatus, directory,
    identity::{ApplicationId, PrincipalId, RoleId},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Forbidden,
    RecentAuthentication,
    NotFound,
    Conflict,
    LastAdministrator,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub status: Option<AccountStatus>,
    pub search: String,
    pub after: Option<PrincipalId>,
    pub limit: u16,
}
impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=100).contains(&self.limit)
            || self.search.chars().count() > 100
            || self.search.chars().any(char::is_control)
            || self.search.trim() != self.search
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    pub first: String,
    pub last: String,
}
impl Names {
    pub fn new(first: &str, last: &str) -> Result<Self, Error> {
        Ok(Self {
            first: directory::validate_name(first).map_err(|_| Error::Invalid)?,
            last: directory::validate_name(last).map_err(|_| Error::Invalid)?,
        })
    }
}
#[derive(Debug, Clone)]
pub enum Change {
    Names(Names),
    Status(AccountStatus),
    Role {
        application: ApplicationId,
        role: RoleId,
        assigned: bool,
        policy_revision: u64,
    },
}
pub fn revision(current: u64, expected: u64) -> Result<u64, Error> {
    crate::registration::next_revision(current, expected).map_err(|_| Error::Conflict)
}
pub fn role_change(
    current_revision: u64,
    expected_revision: u64,
    active: bool,
    existing: bool,
    assigned: bool,
) -> Result<bool, Error> {
    revision(current_revision, expected_revision)?;
    if assigned && !active {
        return Err(Error::Invalid);
    }
    Ok(existing != assigned)
}
#[cfg(test)]
#[path = "../tests/unit/admin_directory.rs"]
mod tests;
