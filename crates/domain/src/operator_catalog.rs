//! Bounded catalog reads available to an authenticated platform administrator.
use crate::{admin_catalog::Query, identity::ApplicationId, operator_accounts::Error};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Applications,
    Clients(ApplicationId),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    target: Target,
    query: Query,
}
impl Request {
    pub fn new(target: Target, query: Query) -> Result<Self, Error> {
        query.validate().map_err(|_| Error::Invalid)?;
        if query.limit > 25 {
            return Err(Error::Invalid);
        }
        Ok(Self { target, query })
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn query(&self) -> &Query {
        &self.query
    }
}
#[cfg(test)]
#[path = "../tests/unit/operator_catalog.rs"]
mod tests;
