use super::configuration::Settings;
use darkhorse_application::media::{Asset, Objects};
use darkhorse_domain::{
    identity::AssetId,
    media::{MAX_BYTES, body_size},
    profiles::Error,
};
use futures_util::TryStreamExt;
use object_store::{ObjectStore, ObjectStoreExt, aws::AmazonS3Builder, path::Path};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Default)]
pub struct Storage {
    cache: Arc<Mutex<super::cache::Cache>>,
    inner: Option<Arc<dyn ObjectStore>>,
    pub bucket: Option<String>,
}
impl Storage {
    pub fn new(settings: Settings) -> Result<Self, Error> {
        initialize_tls();
        Self::build(settings)
    }
    fn build(settings: Settings) -> Result<Self, Error> {
        let store = AmazonS3Builder::new()
            .with_endpoint(settings.endpoint)
            .with_bucket_name(&settings.bucket)
            .with_region(settings.region)
            .with_access_key_id(settings.access_key.as_str())
            .with_secret_access_key(settings.secret_key.as_str())
            .with_virtual_hosted_style_request(false)
            .with_client_options(
                object_store::ClientOptions::new()
                    .with_allow_http(settings.insecure)
                    .with_timeout(Duration::from_secs(5))
                    .with_connect_timeout(Duration::from_secs(2)),
            )
            .with_retry(object_store::RetryConfig {
                max_retries: 0,
                retry_timeout: Duration::from_secs(5),
                ..Default::default()
            })
            .build()
            .map_err(|_| Error::Invalid)?;
        Ok(Self {
            inner: Some(Arc::new(store)),
            cache: Default::default(),
            bucket: Some(settings.bucket),
        })
    }
    pub fn enabled(&self) -> bool {
        self.inner.is_some()
    }
    fn inner(&self) -> Result<&dyn ObjectStore, Error> {
        self.inner.as_deref().ok_or(Error::Unavailable)
    }
}
fn initialize_tls() {
    // The process-wide provider is installed at most once. Other adapters can
    // have installed the same provider before storage is constructed.
    let _ = rustls::crypto::ring::default_provider().install_default();
}
pub fn fingerprint(s: &Settings) -> [u8; 32] {
    Sha256::digest(format!(
        "darkhorse-object-storage-v1\0{}\0{}\0{}",
        s.endpoint, s.bucket, s.region
    ))
    .into()
}
fn path(id: AssetId) -> Path {
    Path::from(format!(
        "darkhorse/media/{}.png",
        uuid::Uuid::from_u128(id.as_u128()).simple()
    ))
}
impl Objects for Storage {
    async fn put(&self, id: AssetId, data: &[u8]) -> Result<(), Error> {
        body_size(data.len())?;
        self.cache
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(id);
        self.inner()?
            .put(&path(id), data.to_vec().into())
            .await
            .map_err(|_| Error::Unavailable)?;
        Ok(())
    }
    async fn get(&self, asset: &Asset) -> Result<Vec<u8>, Error> {
        body_size(asset.bytes)?;
        if let Some(bytes) = self
            .cache
            .lock()
            .map_err(|_| Error::Unavailable)?
            .get(asset)
        {
            return Ok(bytes);
        }
        let result = self
            .inner()?
            .get(&path(asset.id))
            .await
            .map_err(|_| Error::Unavailable)?;
        if result.meta.size != asset.bytes as u64 {
            return Err(Error::Unavailable);
        }
        let mut stream = result.into_stream();
        let mut bytes = Vec::with_capacity(asset.bytes);
        while let Some(chunk) = stream.try_next().await.map_err(|_| Error::Unavailable)? {
            if chunk.len() > MAX_BYTES - bytes.len() || chunk.len() > asset.bytes - bytes.len() {
                return Err(Error::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        let bytes = verify(bytes, asset)?;
        self.cache
            .lock()
            .map_err(|_| Error::Unavailable)?
            .insert(asset.clone(), bytes.clone());
        Ok(bytes)
    }
    async fn delete(&self, id: AssetId) -> Result<(), Error> {
        self.cache
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(id);
        self.inner()?
            .delete(&path(id))
            .await
            .map_err(|_| Error::Unavailable)
    }
}
fn verify(bytes: Vec<u8>, asset: &Asset) -> Result<Vec<u8>, Error> {
    if bytes.len() != asset.bytes || Sha256::digest(&bytes).as_slice() != asset.digest {
        Err(Error::Unavailable)
    } else {
        Ok(bytes)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/media/objects.rs"]
mod tests;
