//! Session-bound refresh rotation; all time and persistent facts are explicit.
use crate::tokens::{self, Error};
pub const FAMILY_MS: u64 = crate::authentication::ABSOLUTE_MS;
pub const IDLE_MS: u64 = crate::authentication::IDLE_MS;
pub const MAX_GENERATION: u16 = 255;
pub const RETENTION_MS: u64 = 24 * 60 * 60 * 1000;
pub const CLEANUP_BATCH: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    pub created: u64,
    pub expires: u64,
    pub revoked: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub created: u64,
    pub expires: u64,
    pub generation: u16,
    pub consumed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub generation: u16,
    pub refresh_expires: u64,
    pub access_expires: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Next(Window),
    Replay,
}
pub fn initial(authenticated: u64, now: u64) -> Result<Family, Error> {
    let expires = tokens::deadline(authenticated, FAMILY_MS)?;
    if now < authenticated || now >= expires {
        return Err(Error::InvalidGrant);
    }
    Ok(Family {
        created: now,
        expires,
        revoked: false,
    })
}
pub fn window(family: &Family, generation: u16, now: u64) -> Result<Window, Error> {
    Ok(Window {
        generation,
        refresh_expires: tokens::deadline(now, IDLE_MS)?.min(family.expires),
        access_expires: tokens::deadline(now, tokens::ACCESS_MS)?.min(family.expires),
    })
}
pub fn rotate(family: &Family, member: &Member, now: u64) -> Result<Step, Error> {
    if family.revoked
        || family.expires <= family.created
        || family.expires - family.created > FAMILY_MS
        || now < family.created
        || now >= family.expires
        || member.created < family.created
        || member.created > now
        || member.expires <= member.created
        || member.expires > family.expires
        || member.expires - member.created > IDLE_MS
        || member.generation > MAX_GENERATION
    {
        return Err(Error::InvalidGrant);
    }
    // Retained consumed verifiers detect replay even after their own idle expiry.
    if member.consumed {
        return Ok(Step::Replay);
    }
    if now >= member.expires || member.generation == MAX_GENERATION {
        return Err(Error::InvalidGrant);
    }
    window(family, member.generation + 1, now).map(Step::Next)
}
pub fn scopes(
    current: &[String],
    requested: Option<&[String]>,
    resource: Option<&str>,
) -> Result<Vec<String>, Error> {
    tokens::profile(current, resource)?;
    let selected = requested.unwrap_or(current);
    tokens::profile(selected, resource)?;
    if selected.iter().any(|scope| !current.contains(scope)) {
        return Err(Error::InvalidScope);
    }
    Ok(selected.to_vec())
}
pub fn retention_cutoff(now: u64) -> Option<u64> {
    now.checked_sub(RETENTION_MS)
}
#[cfg(test)]
#[path = "../tests/unit/refresh.rs"]
mod tests;
