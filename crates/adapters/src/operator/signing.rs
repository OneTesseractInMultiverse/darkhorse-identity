use super::output::{Failure, Output};
use crate::{
    configuration,
    signing::{configuration as settings, crypto},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_application::signing::{KeyInventory, SigningStore};
use darkhorse_application::signing_operations::{self, Intent, Journal, Kind};
use darkhorse_domain::{
    identity::OperationId,
    signing::{KeyError, Phase},
};
mod journal;
use std::io::Read;
use zeroize::Zeroizing;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Status,
    Inspect(OperationId),
    Generate(u64),
    Import(u64),
    Activate { kid: [u8; 32], revision: u64 },
    Retire { kid: [u8; 32], revision: u64 },
}
pub(super) fn identifier(value: &str) -> Result<[u8; 32], &'static str> {
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("Invalid signing key identifier.")
}
pub async fn run(operation: Operation) -> Result<Output, Failure> {
    if let Operation::Inspect(id) = operation {
        let store = super::connect().await?;
        let result = store.inspect_signing(id).await;
        store.close().await;
        return result
            .map(journal::project)
            .map(Output::record)
            .map_err(|error| journal::failure(error, id));
    }
    let http = configuration::load(crate::deployment_environment::DeploymentEnvironment)
        .map_err(|_| "Invalid provider origin.")?;
    let wrap = settings::load(crate::deployment_environment::DeploymentEnvironment)
        .map_err(message)?
        .ok_or("Provider must be explicitly enabled.")?;
    let issuer = http.public_origin.origin().ascii_serialization();
    let store = super::connect().await?;
    let result = execute(&store, &issuer, &wrap, operation).await;
    store.close().await;
    result.map(Output::record)
}
async fn execute(
    store: &(impl SigningStore + Journal),
    issuer: &str,
    wrap: &settings::WrapKey,
    operation: Operation,
) -> Result<serde_json::Value, Failure> {
    match operation {
        Operation::Status => {
            store
                .bind_provider(issuer, wrap.fingerprint())
                .await
                .map_err(message)?;
            Ok(project(store.inventory(issuer).await.map_err(message)?))
        }
        Operation::Generate(revision) => {
            let key = crypto::generate(issuer, wrap).map_err(message)?;
            change(
                store,
                issuer,
                wrap,
                Kind::Generate,
                revision,
                key.public.kid.clone(),
                Some(key),
            )
            .await
        }
        Operation::Import(revision) => {
            let der = read_import(std::io::stdin().lock()).map_err(message)?;
            let key = crypto::import(issuer, wrap, &der).map_err(message)?;
            change(
                store,
                issuer,
                wrap,
                Kind::Import,
                revision,
                key.public.kid.clone(),
                Some(key),
            )
            .await
        }
        Operation::Activate { kid, revision } => {
            change(
                store,
                issuer,
                wrap,
                Kind::Activate,
                revision,
                URL_SAFE_NO_PAD.encode(kid),
                None,
            )
            .await
        }
        Operation::Retire { kid, revision } => {
            change(
                store,
                issuer,
                wrap,
                Kind::Retire,
                revision,
                URL_SAFE_NO_PAD.encode(kid),
                None,
            )
            .await
        }
        Operation::Inspect(_) => Err(Failure::usage()),
    }
}
async fn change(
    store: &impl Journal,
    issuer: &str,
    wrap: &settings::WrapKey,
    kind: Kind,
    expected_revision: u64,
    kid: String,
    key: Option<darkhorse_application::signing::WrappedKey>,
) -> Result<serde_json::Value, Failure> {
    let id = super::operation_id()?;
    let intent = Intent {
        id,
        issuer: issuer.to_owned(),
        kid,
        kind,
        expected_revision,
    };
    let revision = signing_operations::execute(store, &intent, wrap.fingerprint(), key)
        .await
        .map_err(|error| journal::failure(error, id))?;
    Ok(journal::completed(&intent, revision))
}

fn read_import(reader: impl Read) -> Result<Zeroizing<Vec<u8>>, KeyError> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| KeyError::Invalid)?;
    if bytes.is_empty() || bytes.len() > 8192 {
        return Err(KeyError::Invalid);
    }
    Ok(bytes)
}
fn project(inventory: KeyInventory) -> serde_json::Value {
    serde_json::json!({"revision":inventory.revision,"keys":inventory.keys.into_iter().map(|key|serde_json::json!({
        "kid":key.public.kid,"phase":match key.state.phase {Phase::Staged=>"staged",Phase::Active=>"active",Phase::Retiring=>"retiring",Phase::Retired=>"retired"},
        "created_ms":key.state.created_ms,"activated_ms":key.state.activated_ms,"verify_until_ms":key.state.verify_until_ms
    })).collect::<Vec<_>>()})
}
fn message(error: KeyError) -> &'static str {
    match error {
        KeyError::Invalid => "Invalid signing key material or lifecycle input.",
        KeyError::Conflict => {
            "Provider state changed, key capacity is full, or configuration conflicts; inspect signing-status."
        }
        KeyError::NotReady => "Signing key publication or verification window has not elapsed.",
        KeyError::NotFound => "Provider or signing key not found.",
        KeyError::Unavailable => "Signing operation unavailable; inspect state before retrying.",
    }
}
#[cfg(test)]
#[path = "../../tests/unit/operator/signing.rs"]
mod tests;
