use super::*;
#[derive(Default)]
struct Memory(Mutex<std::collections::BTreeMap<u128, Vec<u8>>>);
impl Objects for Memory {
    async fn put(&self, id: AssetId, data: &[u8]) -> Result<(), Error> {
        self.0.lock().unwrap().insert(id.as_u128(), data.to_vec());
        Ok(())
    }
    async fn get(&self, asset: &Asset) -> Result<Vec<u8>, Error> {
        self.0
            .lock()
            .unwrap()
            .get(&asset.id.as_u128())
            .cloned()
            .ok_or(Error::Unavailable)
    }
    async fn delete(&self, id: AssetId) -> Result<(), Error> {
        self.0.lock().unwrap().remove(&id.as_u128());
        Ok(())
    }
}
#[tokio::test]
async fn in_memory_objects_are_bounded_integrity_checked_and_separate_from_keys() {
    let store = Storage {
        cache: Default::default(),
        inner: Some(Memory::default()),
        bucket: Some("test".into()),
    };
    let id = AssetId::from_u128(1).unwrap();
    let bytes = b"canonical image bytes";
    let asset = Asset {
        id,
        bytes: bytes.len(),
        digest: Sha256::digest(bytes).into(),
    };
    store.put(id, bytes).await.unwrap();
    assert_eq!(store.get(&asset).await.unwrap(), bytes);
    assert!(store.enabled());
    assert!(
        store
            .get(&Asset {
                bytes: 1,
                ..asset.clone()
            })
            .await
            .is_err()
    );
    assert!(
        store
            .get(&Asset {
                digest: [0; 32],
                ..asset.clone()
            })
            .await
            .is_err()
    );
    assert!(
        verify(
            bytes.to_vec(),
            &Asset {
                bytes: 1,
                ..asset.clone()
            }
        )
        .is_err()
    );
    store.delete(id).await.unwrap();
    assert!(store.get(&asset).await.is_err());
    store.delete(id).await.unwrap();
    assert!(store.put(id, &[]).await.is_err());
    let disabled = Storage::<Memory>::default();
    assert!(!disabled.enabled());
    assert!(disabled.get(&asset).await.is_err());
    assert!(disabled.put(id, bytes).await.is_err());
    assert!(disabled.delete(id).await.is_err());
}

#[tokio::test]
async fn cache_hits_preserve_identity_and_mutations_invalidate_before_effects() {
    let store = Storage {
        inner: Some(Memory::default()),
        ..Default::default()
    };
    let id = AssetId::from_u128(2).unwrap();
    let asset = Asset {
        id,
        bytes: 3,
        digest: Sha256::digest(b"one").into(),
    };
    store.put(id, b"one").await.unwrap();
    assert_eq!(store.get(&asset).await.unwrap(), b"one");
    store.inner().unwrap().delete(id).await.unwrap();
    assert_eq!(store.get(&asset).await.unwrap(), b"one");
    store.put(id, b"two").await.unwrap();
    assert_eq!(store.get(&asset).await, Err(Error::Unavailable));
    let changed = Asset {
        digest: Sha256::digest(b"two").into(),
        ..asset
    };
    assert_eq!(store.get(&changed).await.unwrap(), b"two");
    store.delete(id).await.unwrap();
    assert_eq!(store.get(&changed).await, Err(Error::Unavailable));
    let too_large = Asset {
        bytes: darkhorse_domain::media::MAX_BYTES + 1,
        ..changed
    };
    assert_eq!(store.get(&too_large).await, Err(Error::Invalid));
}
