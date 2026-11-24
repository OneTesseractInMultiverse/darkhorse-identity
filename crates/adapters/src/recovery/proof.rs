use super::bounded;
use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::{Engine, engine::general_purpose::STANDARD};
use darkhorse_application::recovery::Denied;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use uuid::{Uuid, Variant, Version};

const MAX_ENVELOPE: u64 = 16 * 1024;
pub(super) const MAX_ISSUER: usize = 2048;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    request: String,
    signature: String,
    archive_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    action: String,
    deployment: String,
    issuer: String,
    operation: String,
    archive_sha256: String,
}

pub(super) fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| {
        id.get_version() == Some(Version::Random)
            && id.get_variant() == Variant::RFC4122
            && id.to_string() == value
    })
}

pub(super) fn canonical_origin(value: &str) -> bool {
    value.len() <= MAX_ISSUER
        && url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.port() != Some(0)
                && url.origin().ascii_serialization() == value
        })
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Envelope {
    pub(super) fn read(input: impl Read) -> Result<Self, Denied> {
        serde_json::from_slice(&bounded(input, MAX_ENVELOPE)?).map_err(|_| Denied)
    }

    pub(super) fn receipt(
        &self,
        administrator: &[u8; 32],
        deployment: &str,
        issuer: &str,
    ) -> Result<String, Denied> {
        let signature = STANDARD.decode(&self.signature).map_err(|_| Denied)?;
        UnparsedPublicKey::new(&ED25519, administrator)
            .verify(self.request.as_bytes(), &signature)
            .map_err(|_| Denied)?;
        let req: Request = serde_json::from_str(&self.request).map_err(|_| Denied)?;
        if req.action != "darkhorse.recovery.begin.v1"
            || req.deployment != deployment
            || req.issuer != issuer
            || !canonical_uuid(&req.operation)
            || !canonical_digest(&req.archive_sha256)
            || req.archive_sha256 != self.archive_sha256
        {
            return Err(Denied);
        }
        let fingerprint = crate::session_secret::hex(&Sha256::digest(administrator));
        Ok(format!(
            "quarantine-{}-{}-{fingerprint}",
            req.operation, req.archive_sha256
        ))
    }
}
