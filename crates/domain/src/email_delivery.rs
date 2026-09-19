//! Bounded delivery leases and retries from explicit time inputs.
pub const LEASE_MS: u64 = 60_000;
pub const MAX_ATTEMPTS: u16 = 5;
pub fn lease(now: u64, expires: u64) -> u64 {
    now.saturating_add(LEASE_MS).min(expires)
}
pub fn retry(attempt: u16, now: u64, expires: u64) -> Option<u64> {
    if !(1..MAX_ATTEMPTS).contains(&attempt) {
        return None;
    }
    now.checked_add(60_000 << (attempt - 1))
        .filter(|next| *next < expires)
}
