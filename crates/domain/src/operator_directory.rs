//! A CLI page must fit the bounded output record, including escaped names.
use crate::{admin_directory::Query, operator_accounts::Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request(Query);
impl Request {
    pub fn new(query: Query) -> Result<Self, Error> {
        query.validate().map_err(|_| Error::Invalid)?;
        if query.limit > 25 {
            return Err(Error::Invalid);
        }
        Ok(Self(query))
    }
    pub fn query(&self) -> &Query {
        &self.0
    }
}

#[cfg(test)]
#[path = "../tests/unit/operator_directory.rs"]
mod tests;
