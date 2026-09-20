//! Personal-key issuance policy from explicit values; clocks and storage stay outside.
use crate::{authorization::CapabilitySelection, identity::*, registration::Label};
use std::collections::BTreeSet;

pub const DAY_MS: u64 = 86_400_000;
pub const MAX_TIME: u64 = 8_640_000_000_000_000;
pub const PAGE_SIZE: usize = 25;
pub const MAX_RESOURCES: usize = 16;
pub const MAX_CAPABILITIES: usize = 256;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Forbidden,
    RecentAuthenticationRequired,
    NotFound,
    Conflict,
    Limit,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiration {
    Default,
    Days(u16),
    Never,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryPolicy {
    default_days: u16,
    maximum_days: u16,
    allow_never: bool,
}
impl ExpiryPolicy {
    pub fn new(default_days: u16, maximum_days: u16, allow_never: bool) -> Result<Self, Error> {
        if default_days == 0 || default_days > maximum_days || maximum_days > 3650 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            default_days,
            maximum_days,
            allow_never,
        })
    }
    pub fn default_days(self) -> u16 {
        self.default_days
    }
    pub fn maximum_days(self) -> u16 {
        self.maximum_days
    }
    pub fn allow_never(self) -> bool {
        self.allow_never
    }
    pub fn deadline(self, expiration: Expiration, now: u64) -> Result<Option<u64>, Error> {
        if now > MAX_TIME {
            return Err(Error::Invalid);
        }
        let days = match expiration {
            Expiration::Default => self.default_days,
            Expiration::Never if self.allow_never => return Ok(None),
            Expiration::Never => return Err(Error::Forbidden),
            Expiration::Days(days) => days,
        };
        if days == 0 || days > self.maximum_days {
            return Err(Error::Invalid);
        }
        now.checked_add(u64::from(days) * DAY_MS)
            .filter(|n| *n <= MAX_TIME)
            .map(Some)
            .ok_or(Error::Invalid)
    }
}
#[derive(Debug, Clone)]
pub struct Selection {
    pub resource: ResourceId,
    pub capabilities: CapabilitySelection,
}
#[derive(Debug, Clone)]
pub struct Request {
    name: Label,
    application: ApplicationId,
    revision: u64,
    expiration: Expiration,
    grants: Vec<Selection>,
}
impl Request {
    pub fn new(
        name: &str,
        application: ApplicationId,
        revision: u64,
        expiration: Expiration,
        grants: Vec<Selection>,
    ) -> Result<Self, Error> {
        let name = Label::new(name).map_err(|_| Error::Invalid)?;
        if revision > i64::MAX as u64
            || grants.is_empty()
            || grants.len() > MAX_RESOURCES
            || grants
                .iter()
                .map(|g| g.resource)
                .collect::<BTreeSet<_>>()
                .len()
                != grants.len()
        {
            return Err(Error::Invalid);
        }
        for grant in &grants {
            if let CapabilitySelection::Subset(caps) = &grant.capabilities
                && (caps.is_empty() || caps.len() > MAX_CAPABILITIES)
            {
                return Err(Error::Invalid);
            }
        }
        Ok(Self {
            name,
            application,
            revision,
            expiration,
            grants,
        })
    }
    pub fn name(&self) -> &Label {
        &self.name
    }
    pub fn application(&self) -> ApplicationId {
        self.application
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn expiration(&self) -> Expiration {
        self.expiration
    }
    pub fn grants(&self) -> &[Selection] {
        &self.grants
    }
}
pub fn recent(authenticated_ms: u64, now: u64) -> Result<(), Error> {
    if now
        .checked_sub(authenticated_ms)
        .is_some_and(|age| age < 300_000)
    {
        Ok(())
    } else {
        Err(Error::RecentAuthenticationRequired)
    }
}
pub fn revision(current: u64, expected: u64) -> Result<(), Error> {
    if current == expected {
        Ok(())
    } else {
        Err(Error::Conflict)
    }
}
pub fn capacity(live_keys: usize, recent_keys: usize) -> Result<(), Error> {
    if live_keys >= 100 || recent_keys >= 10 {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
pub fn live(created: u64, expires: Option<u64>, now: u64) -> bool {
    now >= created && expires.is_none_or(|expiry| expiry > created && now < expiry)
}
#[cfg(test)]
#[path = "../tests/unit/personal_keys.rs"]
mod tests;
