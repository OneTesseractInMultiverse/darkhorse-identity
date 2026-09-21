use super::*;
#[tokio::test]
async fn in_memory_objects_are_bounded_integrity_checked_and_separate_from_keys() {
    let store = Storage {
        cache: Default::default(),
        inner: Some(Arc::new(object_store::memory::InMemory::new())),
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
    let disabled = Storage::default();
    assert!(!disabled.enabled());
    assert!(disabled.get(&asset).await.is_err());
    assert!(disabled.put(id, bytes).await.is_err());
    assert!(disabled.delete(id).await.is_err());
}
