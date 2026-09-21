//! Bounded media policy, independent of codecs and cloud providers.
use crate::profiles::Error;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 2048;
pub const UPLOAD_MS: u64 = 60_000;
pub const CLEANUP_GRACE_MS: u64 = 3_600_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Portrait,
    Logo,
    Background,
}
impl Kind {
    pub fn edge(self) -> u32 {
        match self {
            Self::Portrait => 512,
            Self::Logo => 1024,
            Self::Background => 1920,
        }
    }
}
pub fn dimensions(width: u32, height: u32) -> Result<(), Error> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
pub fn body_size(bytes: usize) -> Result<(), Error> {
    if bytes == 0 || bytes > MAX_BYTES {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
pub fn attachable(created: u64, now: u64, state: &str) -> Result<(), Error> {
    if state != "pending" || now < created || now - created >= UPLOAD_MS {
        Err(Error::Conflict)
    } else {
        Ok(())
    }
}
pub fn capacity(recent: u64) -> Result<(), Error> {
    if recent >= 4 {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
#[cfg(test)]
#[path = "../tests/unit/media.rs"]
mod tests;
