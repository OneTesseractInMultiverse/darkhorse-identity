use super::*;
fn asset(id: u128, bytes: usize) -> Asset {
    Asset {
        id: AssetId::from_u128(id).unwrap(),
        bytes,
        digest: [1; 32],
    }
}
#[test]
fn immutable_byte_cache_is_bounded_and_never_matches_changed_metadata() {
    let mut cache = Cache::default();
    let key = asset(1, 1);
    assert!(cache.get(&key).is_none());
    cache.insert(key.clone(), vec![1]);
    assert_eq!(cache.get(&key), Some(vec![1]));
    assert!(
        cache
            .get(&Asset {
                digest: [2; 32],
                ..key.clone()
            })
            .is_none()
    );
    cache.remove(key.id);
    assert!(cache.get(&key).is_none());
    cache.remove(key.id);
    for id in 1..=33 {
        cache.insert(asset(id, 1), vec![1]);
    }
    assert_eq!(cache.entries.len(), 32);
    assert!(cache.get(&key).is_none());
    cache.insert(asset(99, BYTE_LIMIT), vec![1; BYTE_LIMIT]);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.bytes, BYTE_LIMIT);
    cache.insert(asset(99, 2), vec![1; 2]);
    assert_eq!(cache.bytes, 2);
    cache.insert(asset(100, BYTE_LIMIT + 1), vec![1; BYTE_LIMIT + 1]);
    assert_eq!(cache.bytes, 2);
}
