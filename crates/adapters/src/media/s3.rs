use super::{configuration::Settings, s3_signing};
use axum::{
    body::Bytes,
    http::{Method, StatusCode},
};
use darkhorse_application::media::{Asset, Objects};
use darkhorse_domain::{identity::AssetId, media::body_size, profiles::Error};
use futures_util::{Stream, TryStreamExt};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    settings: Arc<Settings>,
}
impl Client {
    pub(super) fn new(settings: Settings) -> Result<Self, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = http_client(settings.insecure)?;
        Ok(Self {
            http,
            settings: Arc::new(settings),
        })
    }
    async fn send(
        &self,
        method: Method,
        id: AssetId,
        bytes: &[u8],
    ) -> Result<reqwest::Response, Error> {
        let request = s3_signing::request(&self.settings, method, id, bytes, SystemTime::now())?;
        self.http
            .execute(request)
            .await
            .map_err(|_| Error::Unavailable)
    }
}
fn http_client(insecure: bool) -> Result<reqwest::Client, Error> {
    reqwest::Client::builder()
        .https_only(!insecure)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_gzip()
        .no_brotli()
        .no_zstd()
        .no_deflate()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .tls_version_min(reqwest::tls::Version::TLS_1_2)
        .tls_sslkeylogfile(false)
        .build()
        .map_err(|_| Error::Invalid)
}
impl Objects for Client {
    async fn put(&self, id: AssetId, data: &[u8]) -> Result<(), Error> {
        body_size(data.len())?;
        let response = self.send(Method::PUT, id, data).await?;
        mutation_status(response.status(), false)
    }
    async fn get(&self, asset: &Asset) -> Result<Vec<u8>, Error> {
        body_size(asset.bytes)?;
        let response = self.send(Method::GET, asset.id, &[]).await?;
        read_status(response.status(), response.content_length(), asset.bytes)?;
        read_body(response.bytes_stream(), asset.bytes).await
    }
    async fn delete(&self, id: AssetId) -> Result<(), Error> {
        let response = self.send(Method::DELETE, id, &[]).await?;
        mutation_status(response.status(), true)
    }
}
fn mutation_status(status: StatusCode, delete: bool) -> Result<(), Error> {
    if status == StatusCode::OK
        || status == StatusCode::NO_CONTENT
        || (delete && status == StatusCode::NOT_FOUND)
    {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}
fn read_status(status: StatusCode, length: Option<u64>, expected: usize) -> Result<(), Error> {
    if status == StatusCode::OK && length == Some(expected as u64) {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}
async fn read_body<S, E>(mut stream: S, expected: usize) -> Result<Vec<u8>, Error>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut bytes = Vec::with_capacity(expected);
    while let Some(chunk) = stream.try_next().await.map_err(|_| Error::Unavailable)? {
        append(&mut bytes, &chunk, expected)?;
    }
    Ok(bytes)
}
fn append(bytes: &mut Vec<u8>, chunk: &[u8], expected: usize) -> Result<(), Error> {
    if chunk.len() > expected.saturating_sub(bytes.len()) {
        return Err(Error::Unavailable);
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/media/s3.rs"]
mod tests;
