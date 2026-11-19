use super::configuration::Settings;
use darkhorse_application::media::{Asset, Objects};
use darkhorse_domain::{identity::AssetId, media::body_size, profiles::Error};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
pub struct Storage<T = super::s3::Client> {
    cache: Arc<Mutex<super::cache::Cache>>,
    inner: Option<T>,
    pub bucket: Option<String>,
}
impl<T> Default for Storage<T> {
    fn default() -> Self {
        Self {
            cache: Default::default(),
            inner: None,
            bucket: None,
        }
    }
}
impl Storage {
    pub fn new(settings: Settings) -> Result<Self, Error> {
        let bucket = settings.bucket.clone();
        let store = super::s3::Client::new(settings)?;
        Ok(Self {
            inner: Some(store),
            cache: Default::default(),
            bucket: Some(bucket),
        })
    }
}
impl<T> Storage<T> {
    pub fn enabled(&self) -> bool {
        self.inner.is_some()
    }
    fn inner(&self) -> Result<&T, Error> {
        self.inner.as_ref().ok_or(Error::Unavailable)
    }
}
pub fn fingerprint(s: &Settings) -> [u8; 32] {
    Sha256::digest(format!(
        "darkhorse-object-storage-v1\0{}\0{}\0{}",
        s.endpoint, s.bucket, s.region
    ))
    .into()
}
impl<T: Objects> Objects for Storage<T> {
    async fn put(&self, id: AssetId, data: &[u8]) -> Result<(), Error> {
        body_size(data.len())?;
        self.cache
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(id);
        self.inner()?.put(id, data).await
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
        let bytes = self.inner()?.get(asset).await?;
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
        self.inner()?.delete(id).await
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
