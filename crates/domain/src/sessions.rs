//! Self-service session policy. Public references are never login credentials.
use crate::{
    authentication::{SessionFacts, session_live},
    identity::{PrincipalId, SessionId},
};
pub const PAGE_SIZE: usize = 25;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    NotFound,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Inactive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub created_ms: u64,
    pub id: SessionId,
}
impl Cursor {
    pub fn new(created_ms: u64, id: SessionId) -> Result<Self, Error> {
        if created_ms > i64::MAX as u64 {
            return Err(Error::Invalid);
        }
        Ok(Self { created_ms, id })
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub id: SessionId,
    pub created_ms: u64,
    pub seen_ms: u64,
    pub expires_ms: u64,
    pub status: Status,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    pub current: SessionId,
    pub items: Vec<Record>,
    pub next: Option<Cursor>,
}
pub fn authenticate(facts: SessionFacts, now: u64) -> Result<(), Error> {
    if !session_live(facts, now) {
        return Err(Error::Unauthorized);
    }
    Ok(())
}
pub fn owns(actor: PrincipalId, target: PrincipalId) -> Result<(), Error> {
    if actor != target {
        return Err(Error::NotFound);
    }
    Ok(())
}
pub fn status(facts: SessionFacts, now: u64) -> Status {
    if session_live(facts, now) {
        Status::Active
    } else {
        Status::Inactive
    }
}
pub fn page(current: SessionId, mut items: Vec<Record>) -> Result<Page, Error> {
    if items.len() > PAGE_SIZE + 1 {
        return Err(Error::Unavailable);
    }
    let more = items.len() > PAGE_SIZE;
    items.truncate(PAGE_SIZE);
    let next = if more {
        items.last().map(|r| Cursor {
            created_ms: r.created_ms,
            id: r.id,
        })
    } else {
        None
    };
    Ok(Page {
        current,
        items,
        next,
    })
}
#[cfg(test)]
#[path = "../tests/unit/sessions.rs"]
mod tests;
