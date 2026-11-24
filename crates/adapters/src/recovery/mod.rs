//! Local recovery custody, independent of the archived database.
mod control;
mod proof;

use control::Control;
use darkhorse_application::recovery::{Action, Decision, Denied, decide};
use nix::fcntl::renameat;
use std::{env, fs::File, io::Read, path::Path};

const CONTROL_DIR: &str = "/run/darkhorse-recovery";
const STARTUP_DENIED: &str = "Recovery control denies startup.";
const REQUEST_DENIED: &str = "Recovery request denied.";
const UNCERTAIN: &str = "Recovery outcome uncertain.";

/// The caller holds the shared lock until every serving task and worker has stopped.
pub fn serving_lease(managed: bool) -> Result<Option<File>, &'static str> {
    let required = managed || env::var_os("DARKHORSE_RECOVERY_REQUIRED").is_some();
    let issuer = env::var("DARKHORSE_PUBLIC_ORIGIN").ok();
    serving_at(Path::new(CONTROL_DIR), required, issuer.as_deref())
}

fn serving_at(
    path: &Path,
    required: bool,
    issuer: Option<&str>,
) -> Result<Option<File>, &'static str> {
    let Some(dir) = control::directory(path).map_err(|_| STARTUP_DENIED)? else {
        return if required {
            Err(STARTUP_DENIED)
        } else {
            Ok(None)
        };
    };
    dir.try_lock_shared().map_err(|_| STARTUP_DENIED)?;
    let state = Control::read(&dir, issuer.ok_or(STARTUP_DENIED)?).map_err(|_| STARTUP_DENIED)?;
    match decide(state.serving, state.receipt.as_deref(), Action::Serve) {
        Ok(Decision::Serve) => Ok(Some(dir)),
        _ => Err(STARTUP_DENIED),
    }
}

/// Authenticate and durably quarantine. A retry must prove the original binding again.
pub fn begin() -> Result<(), &'static str> {
    let issuer = env::var("DARKHORSE_PUBLIC_ORIGIN").ok();
    begin_at(
        Path::new(CONTROL_DIR),
        issuer.as_deref(),
        std::io::stdin().lock(),
    )
}

fn begin_at(path: &Path, issuer: Option<&str>, input: impl Read) -> Result<(), &'static str> {
    let issuer = issuer.ok_or(REQUEST_DENIED)?;
    let envelope = proof::Envelope::read(input).map_err(|_| REQUEST_DENIED)?;
    let dir = control::directory(path)
        .map_err(|_| REQUEST_DENIED)?
        .ok_or(REQUEST_DENIED)?;
    dir.try_lock().map_err(|_| REQUEST_DENIED)?;
    let state = Control::read(&dir, issuer).map_err(|_| REQUEST_DENIED)?;
    let administrator = state.administrator.as_ref().ok_or(REQUEST_DENIED)?;
    let receipt = envelope
        .receipt(administrator, &state.deployment, issuer)
        .map_err(|_| REQUEST_DENIED)?;
    match decide(
        state.serving,
        state.receipt.as_deref(),
        Action::Begin(&receipt),
    ) {
        Ok(Decision::Fence) => {
            renameat(&dir, "serving", &dir, receipt.as_str()).map_err(|_| UNCERTAIN)?
        }
        Ok(Decision::Confirm) => {}
        _ => return Err(REQUEST_DENIED),
    }
    // A prior process may have exited between rename and sync. Retry syncs too.
    dir.sync_all().map_err(|_| UNCERTAIN)
}

fn bounded(input: impl Read, limit: u64) -> Result<Vec<u8>, Denied> {
    let mut bytes = Vec::new();
    input
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Denied)?;
    if bytes.len() as u64 > limit {
        return Err(Denied);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "../../tests/unit/recovery/mod.rs"]
mod tests;
