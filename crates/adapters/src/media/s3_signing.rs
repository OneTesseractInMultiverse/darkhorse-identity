use super::configuration::Settings;
use aws_credential_types::Credentials;
use aws_sigv4::{
    http_request::{
        PayloadChecksumKind, PercentEncodingMode, SignableBody, SignableRequest, SigningSettings,
        UriPathNormalizationMode, sign,
    },
    sign::v4,
};
use axum::http::{Method, Request};
use darkhorse_domain::{identity::AssetId, profiles::Error};
use sha2::{Digest, Sha256};
use std::time::SystemTime;

pub(super) fn request(
    settings: &Settings,
    method: Method,
    id: AssetId,
    bytes: &[u8],
    now: SystemTime,
) -> Result<reqwest::Request, Error> {
    let uri = object_url(settings, id)?;
    let identity = Credentials::new(
        settings.access_key.as_str(),
        settings.secret_key.as_str(),
        None,
        None,
        "darkhorse",
    )
    .into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(&settings.region)
        .name("s3")
        .time(now)
        .settings(signing_settings())
        .build()
        .map_err(|_| Error::Unavailable)?
        .into();
    let signable = SignableRequest::new(
        method.as_str(),
        uri.as_str(),
        std::iter::empty(),
        // Keep image bytes out of the SDK's optional request trace formatting.
        SignableBody::Precomputed(payload_digest(bytes)),
    )
    .map_err(|_| Error::Unavailable)?;
    let (instructions, _) = sign(signable, &params)
        .map_err(|_| Error::Unavailable)?
        .into_parts();
    let mut request = Request::builder()
        .method(method)
        .uri(uri.as_str())
        .body(bytes.to_vec())
        .map_err(|_| Error::Unavailable)?;
    instructions.apply_to_request_http1x(&mut request);
    request
        .headers_mut()
        .get_mut(axum::http::header::AUTHORIZATION)
        .ok_or(Error::Unavailable)?
        .set_sensitive(true);
    reqwest::Request::try_from(request).map_err(|_| Error::Unavailable)
}

fn payload_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn signing_settings() -> SigningSettings {
    let mut settings = SigningSettings::default();
    settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    settings
}

fn object_url(settings: &Settings, id: AssetId) -> Result<url::Url, Error> {
    let mut url = url::Url::parse(&settings.endpoint).map_err(|_| Error::Invalid)?;
    url.set_path(&format!(
        "/{}/darkhorse/media/{}.png",
        settings.bucket,
        uuid::Uuid::from_u128(id.as_u128()).simple()
    ));
    Ok(url)
}

#[cfg(test)]
#[path = "../../tests/unit/media/s3_signing.rs"]
mod tests;
