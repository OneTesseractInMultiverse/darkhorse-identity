use super::*;
use darkhorse_domain::{
    admin_catalog::Change,
    identity::{ApplicationId, PrincipalId, RoleId},
};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use zeroize::Zeroizing;

const INPUT_LIMIT: usize = 32 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtectedInput {
    authentication: authenticated::Input,
    #[serde(deserialize_with = "crate::registration_http::input::revision")]
    policy_revision: u64,
    change: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrincipalRoleInput {
    operation: String,
    principal_id: String,
    application_id: String,
    role_id: String,
    assigned: bool,
    #[serde(deserialize_with = "crate::registration_http::input::revision")]
    principal_revision: u64,
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
    let change = change(input.change)?;
    let reason = input.authentication.reason.as_deref().unwrap_or_default();
    let request = MutationRequest::new(input.policy_revision, change, reason)
        .map_err(|_| Failure::usage())?;
    Ok((request, input.authentication))
}

fn change(value: Value) -> Result<Change, Failure> {
    if value.get("operation").and_then(Value::as_str) == Some("principal_role") {
        let input: PrincipalRoleInput =
            serde_json::from_value(value).map_err(|_| Failure::usage())?;
        if input.operation != "principal_role" {
            return Err(Failure::usage());
        }
        return Ok(Change::PrincipalRole {
            principal: crate::registration_http::input::id(
                &input.principal_id,
                PrincipalId::from_u128,
            )
            .map_err(|_| Failure::usage())?,
            application: crate::registration_http::input::id(
                &input.application_id,
                ApplicationId::from_u128,
            )
            .map_err(|_| Failure::usage())?,
            role: crate::registration_http::input::id(&input.role_id, RoleId::from_u128)
                .map_err(|_| Failure::usage())?,
            assigned: input.assigned,
            principal_revision: input.principal_revision,
        });
    }
    let input: crate::admin_catalog_http::input::Input =
        serde_json::from_value(value).map_err(|_| Failure::usage())?;
    input.change().map_err(|_| Failure::usage())
}

#[cfg(test)]
#[path = "../../../tests/unit/operator/access_catalog_mutations/input.rs"]
mod tests;
