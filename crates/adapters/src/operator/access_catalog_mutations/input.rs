use super::*;
use serde::Deserialize;
use std::io::Read;
use zeroize::Zeroizing;

const INPUT_LIMIT: usize = 32 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtectedInput {
    authentication: authenticated::Input,
    #[serde(deserialize_with = "crate::registration_http::input::revision")]
    policy_revision: u64,
    change: crate::admin_catalog_http::input::Input,
}

pub(super) fn read(reader: impl Read) -> Result<(MutationRequest, authenticated::Input), Failure> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::usage())?;
    if bytes.len() > INPUT_LIMIT {
        return Err(Failure::usage());
    }
    let input: ProtectedInput = serde_json::from_slice(&bytes).map_err(|_| Failure::usage())?;
    let change = input.change.change().map_err(|_| Failure::usage())?;
    let reason = input.authentication.reason.as_deref().unwrap_or_default();
    let request = MutationRequest::new(input.policy_revision, change, reason)
        .map_err(|_| Failure::usage())?;
    Ok((request, input.authentication))
}

#[cfg(test)]
#[path = "../../../tests/unit/operator/access_catalog_mutations/input.rs"]
mod tests;
