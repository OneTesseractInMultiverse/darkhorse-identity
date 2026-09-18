//! Signing-key lifecycle policy from explicit primary-state facts.
pub const PREPUBLICATION_MS: u64 = 60_000;
pub const VERIFICATION_OVERLAP_MS: u64 = 600_000;
pub const MAX_TOKEN_SECONDS: u64 = 300;
pub const MAX_PUBLISHED_KEYS: usize = 4;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Invalid,
    NotReady,
    Conflict,
    NotFound,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Staged,
    Active,
    Retiring,
    Retired,
}
#[derive(Debug, Clone, Copy)]
pub struct KeyState {
    pub phase: Phase,
    pub created_ms: u64,
    pub activated_ms: Option<u64>,
    pub verify_until_ms: Option<u64>,
}
#[derive(Debug, Clone, Copy)]
pub enum Purpose {
    IdToken,
    LogoutToken,
}
impl Purpose {
    pub fn header_type(self) -> &'static str {
        match self {
            Self::IdToken => "JWT",
            Self::LogoutToken => "logout+jwt",
        }
    }
}
pub fn activate(state: KeyState, now: u64) -> Result<(), KeyError> {
    if now < state.created_ms {
        return Err(KeyError::Invalid);
    }
    if state.phase != Phase::Staged {
        return Err(KeyError::Conflict);
    }
    if now - state.created_ms < PREPUBLICATION_MS {
        return Err(KeyError::NotReady);
    }
    Ok(())
}
pub fn overlap_end(now: u64) -> Result<u64, KeyError> {
    now.checked_add(VERIFICATION_OVERLAP_MS)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(KeyError::Invalid)
}
pub fn retire(state: KeyState, now: u64) -> Result<(), KeyError> {
    if now < state.created_ms {
        return Err(KeyError::Invalid);
    }
    match state.phase {
        Phase::Staged => Ok(()),
        Phase::Retiring if state.verify_until_ms.is_some_and(|end| now >= end) => Ok(()),
        Phase::Retiring => Err(KeyError::NotReady),
        _ => Err(KeyError::Conflict),
    }
}
pub fn published(state: KeyState, now: u64) -> bool {
    now >= state.created_ms
        && match state.phase {
            Phase::Staged => true,
            Phase::Active => state.activated_ms.is_some_and(|start| now >= start),
            Phase::Retiring => {
                state.activated_ms.is_some_and(|start| now >= start)
                    && state.verify_until_ms.is_some_and(|end| now < end)
            }
            Phase::Retired => false,
        }
}
pub fn message_times(issued: u64, expires: u64) -> Result<(), KeyError> {
    if expires <= issued || expires - issued > MAX_TOKEN_SECONDS {
        return Err(KeyError::Invalid);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/signing.rs"]
mod tests;
