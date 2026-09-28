use super::KEY_PREFIX;
use darkhorse_application::resource_servers::{
    ComputationCacheContext, ComputationCacheEntry, ComputationCacheKey,
};
use darkhorse_domain::identity::*;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;
pub(crate) const MAX_VALUE_BYTES: u64 = 65_536;
pub(crate) const MAX_PAYLOAD_BYTES: usize = 48 * 1024;
const MAX_SCOPES: usize = 64;
const MAX_SCOPE_BYTES: usize = 256;

pub(crate) enum Read {
    Hit(ComputationCacheEntry),
    Miss,
    Invalid,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Signed {
    entry: Unsigned,
    mac: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unsigned {
    policy_revision: u64,
    context: Context,
    payload: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    token_digest: String,
    resource: String,
    principal: String,
    client: String,
    credential: String,
    principal_epoch: u64,
    scopes: Vec<String>,
    capability_ceiling: Vec<String>,
}

pub(crate) fn key(value: &ComputationCacheKey) -> String {
    format!(
        "{KEY_PREFIX}:{}:{}",
        Uuid::from_u128(value.resource.as_u128()),
        hex(&value.token_digest)
    )
}

pub(crate) fn encode(
    key: &ComputationCacheKey,
    entry: &ComputationCacheEntry,
    mac_key: &[u8; 32],
) -> Result<Vec<u8>, ()> {
    if entry.context.resource != key.resource
        || entry.context.token_digest != key.token_digest
        || entry.policy_revision > i64::MAX as u64
        || entry.payload.is_empty()
        || entry.payload.len() > MAX_PAYLOAD_BYTES
        || !bounded_context(&entry.context)
    {
        return Err(());
    }
    let unsigned = Unsigned {
        policy_revision: entry.policy_revision,
        context: context(&entry.context),
        payload: base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &entry.payload,
        ),
    };
    let canonical = serde_json::to_vec(&unsigned).map_err(|_| ())?;
    let mac = sign(mac_key, &canonical)?;
    let bytes = serde_json::to_vec(&Signed {
        entry: unsigned,
        mac: hex(&mac),
    })
    .map_err(|_| ())?;
    (bytes.len() <= MAX_VALUE_BYTES as usize)
        .then_some(bytes)
        .ok_or(())
}

pub(crate) fn decode(
    key: &ComputationCacheKey,
    bytes: &[u8],
    mac_key: &[u8; 32],
) -> Result<ComputationCacheEntry, ()> {
    if bytes.is_empty() || bytes.len() > MAX_VALUE_BYTES as usize {
        return Err(());
    }
    let signed: Signed = serde_json::from_slice(bytes).map_err(|_| ())?;
    let canonical = serde_json::to_vec(&signed.entry).map_err(|_| ())?;
    let expected = decode_hex::<32>(&signed.mac)?;
    verify(mac_key, &canonical, &expected)?;
    if signed.entry.policy_revision > i64::MAX as u64
        || signed.entry.payload.len() > encoded_limit(MAX_PAYLOAD_BYTES)
    {
        return Err(());
    }
    let context = parse_context(signed.entry.context)?;
    if context.resource != key.resource || context.token_digest != key.token_digest {
        return Err(());
    }
    let payload = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        signed.entry.payload,
    )
    .map_err(|_| ())?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(());
    }
    Ok(ComputationCacheEntry {
        policy_revision: signed.entry.policy_revision,
        context,
        payload,
    })
}

fn context(value: &ComputationCacheContext) -> Context {
    Context {
        token_digest: hex(&value.token_digest),
        resource: uuid(value.resource.as_u128()),
        principal: uuid(value.principal.as_u128()),
        client: uuid(value.client.as_u128()),
        credential: uuid(value.credential.as_u128()),
        principal_epoch: value.principal_epoch,
        scopes: value.scopes.clone(),
        capability_ceiling: value
            .capability_ceiling
            .iter()
            .map(|id| uuid(id.as_u128()))
            .collect(),
    }
}

fn parse_context(value: Context) -> Result<ComputationCacheContext, ()> {
    if value.scopes.len() > MAX_SCOPES
        || value
            .scopes
            .iter()
            .any(|scope| scope.is_empty() || scope.len() > MAX_SCOPE_BYTES)
        || value.capability_ceiling.len() > 256
    {
        return Err(());
    }
    let ceiling = value
        .capability_ceiling
        .iter()
        .map(|value| CapabilityId::from_u128(parse_uuid(value)?.as_u128()).map_err(|_| ()))
        .collect::<Result<_, _>>()?;
    Ok(ComputationCacheContext {
        token_digest: decode_hex(&value.token_digest)?,
        resource: ResourceId::from_u128(parse_uuid(&value.resource)?.as_u128()).map_err(|_| ())?,
        principal: PrincipalId::from_u128(parse_uuid(&value.principal)?.as_u128())
            .map_err(|_| ())?,
        client: ClientId::from_u128(parse_uuid(&value.client)?.as_u128()).map_err(|_| ())?,
        credential: CredentialId::from_u128(parse_uuid(&value.credential)?.as_u128())
            .map_err(|_| ())?,
        principal_epoch: value.principal_epoch,
        scopes: value.scopes,
        capability_ceiling: ceiling,
    })
}

fn bounded_context(context: &ComputationCacheContext) -> bool {
    context.scopes.len() <= MAX_SCOPES
        && context
            .scopes
            .iter()
            .all(|scope| !scope.is_empty() && scope.len() <= MAX_SCOPE_BYTES)
        && context.capability_ceiling.len() <= 256
}

fn parse_uuid(value: &str) -> Result<Uuid, ()> {
    let parsed = Uuid::parse_str(value).map_err(|_| ())?;
    (parsed.to_string() == value && !parsed.is_nil())
        .then_some(parsed)
        .ok_or(())
}

fn uuid(value: u128) -> String {
    Uuid::from_u128(value).to_string()
}

fn hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(value.len() * 2);
    for byte in value {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], ()> {
    if value.len() != N * 2 {
        return Err(());
    }
    let mut result = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = nibble(pair[0]).ok_or(())?;
        let low = nibble(pair[1]).ok_or(())?;
        result[index] = (high << 4) | low;
    }
    Ok(result)
}

fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn sign(key: &[u8; 32], message: &[u8]) -> Result<[u8; 32], ()> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| ())?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().into())
}

fn verify(key: &[u8; 32], message: &[u8], tag: &[u8; 32]) -> Result<(), ()> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| ())?;
    mac.update(message);
    mac.verify_slice(tag).map_err(|_| ())
}

fn encoded_limit(value: usize) -> usize {
    value.saturating_mul(4).div_ceil(3)
}

#[cfg(test)]
#[path = "../../tests/unit/redis_computation_cache/wire.rs"]
mod tests;
