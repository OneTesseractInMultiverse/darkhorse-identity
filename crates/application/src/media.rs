//! Upload intent is durable before cloud I/O; final publication rechecks authority.
use darkhorse_domain::{
    identity::{AssetId, PrincipalId},
    media::{Kind, body_size},
    profiles::Error,
};
use std::future::Future;
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub kind: Kind,
    pub principal: Option<PrincipalId>,
}
#[derive(Debug, Clone)]
pub struct Asset {
    pub id: AssetId,
    pub bytes: usize,
    pub digest: [u8; 32],
}
#[derive(Debug, Clone)]
pub struct Ticket {
    pub id: AssetId,
    pub target: Target,
}
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub digest: [u8; 32],
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone)]
pub struct Branding {
    pub revision: u64,
    pub logo: bool,
    pub background: bool,
}
pub trait Store: Send + Sync {
    fn reserve(
        &self,
        actor: [u8; 32],
        target: Target,
        revision: u64,
    ) -> impl Future<Output = Result<Ticket, Error>> + Send;
    fn attach(
        &self,
        actor: [u8; 32],
        ticket: Ticket,
        revision: u64,
        asset: &Prepared,
    ) -> impl Future<Output = Result<u64, Error>> + Send;
    fn remove(
        &self,
        actor: [u8; 32],
        target: Target,
        revision: u64,
    ) -> impl Future<Output = Result<u64, Error>> + Send;
    fn asset(
        &self,
        actor: Option<[u8; 32]>,
        target: Target,
    ) -> impl Future<Output = Result<Option<Asset>, Error>> + Send;
    fn branding(&self, actor: [u8; 32]) -> impl Future<Output = Result<Branding, Error>> + Send;
    fn garbage(&self) -> impl Future<Output = Result<Vec<AssetId>, Error>> + Send;
    fn cleaned(&self, id: AssetId) -> impl Future<Output = Result<(), Error>> + Send;
}
pub trait Images: Send + Sync {
    fn prepare(
        &self,
        kind: Kind,
        content_type: String,
        bytes: Vec<u8>,
    ) -> impl Future<Output = Result<Prepared, Error>> + Send;
}
pub trait Objects: Send + Sync {
    fn put(&self, id: AssetId, data: &[u8]) -> impl Future<Output = Result<(), Error>> + Send;
    fn get(&self, asset: &Asset) -> impl Future<Output = Result<Vec<u8>, Error>> + Send;
    fn delete(&self, id: AssetId) -> impl Future<Output = Result<(), Error>> + Send;
}
#[derive(Clone)]
pub struct Service<S, O, I> {
    pub store: S,
    pub objects: O,
    pub images: I,
}
impl<S: Store, O: Objects, I: Images> Service<S, O, I> {
    pub async fn upload(
        &self,
        actor: [u8; 32],
        target: Target,
        revision: u64,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<u64, Error> {
        body_size(bytes.len())?;
        let ticket = self.store.reserve(actor, target, revision).await?;
        let prepared = self
            .images
            .prepare(target.kind, content_type, bytes)
            .await?;
        self.objects.put(ticket.id, &prepared.bytes).await?;
        self.store.attach(actor, ticket, revision, &prepared).await
    }
    pub async fn read(
        &self,
        actor: Option<[u8; 32]>,
        target: Target,
    ) -> Result<Option<Vec<u8>>, Error> {
        let Some(asset) = self.store.asset(actor, target).await? else {
            return Ok(None);
        };
        self.objects.get(&asset).await.map(Some)
    }
    pub async fn sweep(&self) -> Result<(), Error> {
        for id in self.store.garbage().await? {
            self.objects.delete(id).await?;
            self.store.cleaned(id).await?;
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "../tests/unit/media.rs"]
mod tests;
