//! Password login and session validity from explicit facts, independent of transport.
use crate::directory::{DirectoryError, login_email_key};

pub const IDLE_MS: u64 = 15 * 60 * 1000;
pub const ABSOLUTE_MS: u64 = 8 * 60 * 60 * 1000;
pub const MAX_PASSWORD_BYTES: usize = 512;

pub fn login_email(email: &str) -> Result<String, DirectoryError> {
    login_email_key(email)
}
pub fn login_password(password: &str) -> Result<(), DirectoryError> {
    if password.is_empty()
        || password.len() > MAX_PASSWORD_BYTES
        || password.chars().any(char::is_control)
    {
        return Err(DirectoryError::Password);
    }
    Ok(())
}
pub fn authenticated(exists: bool, active: bool, matches: bool) -> bool {
    exists && active && matches
}

#[derive(Clone, Copy)]
pub struct SessionFacts {
    pub active: bool,
    pub credential_live: bool,
    pub revoked: bool,
    pub issued_epoch: u64,
    pub current_epoch: u64,
    pub created_ms: u64,
    pub seen_ms: u64,
    pub expires_ms: u64,
}
pub fn session_live(f: SessionFacts, now_ms: u64) -> bool {
    f.active
        && f.credential_live
        && !f.revoked
        && f.issued_epoch == f.current_epoch
        && f.created_ms <= f.seen_ms
        && f.seen_ms <= now_ms
        && now_ms < f.expires_ms
        && f.expires_ms - f.created_ms <= ABSOLUTE_MS
        && now_ms - f.seen_ms < IDLE_MS
}

#[cfg(test)]
#[path = "../tests/unit/authentication.rs"]
mod tests;
