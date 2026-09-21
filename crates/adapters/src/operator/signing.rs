use crate::{
    configuration,
    signing::{configuration as settings, crypto},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use darkhorse_application::signing::{KeyInventory, SigningStore};
use darkhorse_domain::signing::{KeyError, Phase};
use std::io::Read;
use zeroize::Zeroizing;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Status,
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
pub async fn run(operation: Operation) -> Result<(), &'static str> {
    let http = configuration::load(crate::deployment_environment::DeploymentEnvironment)
        .map_err(|_| "Invalid provider origin.")?;
    let wrap = settings::load(crate::deployment_environment::DeploymentEnvironment)
        .map_err(message)?
        .ok_or("Provider must be explicitly enabled.")?;
    let issuer = http.public_origin.origin().ascii_serialization();
    let store = super::connect().await?;
    store
        .bind_provider(&issuer, wrap.fingerprint())
        .await
        .map_err(message)?;
    let result = execute(&store, &issuer, &wrap, operation).await;
    store.close().await;
    let value = result.map_err(message)?;
    println!("{value}");
    Ok(())
}
async fn execute(
    store: &impl SigningStore,
    issuer: &str,
    wrap: &settings::WrapKey,
    operation: Operation,
) -> Result<serde_json::Value, KeyError> {
    match operation {
        Operation::Status => Ok(project(store.inventory(issuer).await?)),
        Operation::Generate(revision) => {
            let key = crypto::generate(issuer, wrap)?;
            let kid = key.public.kid.clone();
            let revision = store
                .stage(issuer, wrap.fingerprint(), revision, key)
                .await?;
            Ok(serde_json::json!({"kid":kid,"revision":revision,"phase":"staged"}))
        }
        Operation::Import(revision) => {
            let der = read_import(std::io::stdin().lock())?;
            let key = crypto::import(issuer, wrap, &der)?;
            let kid = key.public.kid.clone();
            let revision = store
                .stage(issuer, wrap.fingerprint(), revision, key)
                .await?;
            Ok(serde_json::json!({"kid":kid,"revision":revision,"phase":"staged"}))
        }
        Operation::Activate { kid, revision } => {
            let revision = store
                .activate(issuer, &URL_SAFE_NO_PAD.encode(kid), revision)
                .await?;
            Ok(serde_json::json!({"revision":revision,"phase":"active"}))
        }
        Operation::Retire { kid, revision } => {
            let revision = store
                .retire(issuer, &URL_SAFE_NO_PAD.encode(kid), revision)
                .await?;
            Ok(serde_json::json!({"revision":revision,"phase":"retired"}))
        }
    }
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
