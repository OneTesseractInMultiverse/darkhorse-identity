use super::*;
use darkhorse_application::resource_servers::{
    ComputationCacheContext, ComputationCacheEntry, ComputationCacheKey,
};
use uuid::Uuid;

fn key() -> ComputationCacheKey {
    ComputationCacheKey {
        resource: ResourceId::from_u128(1).unwrap(),
        token_digest: [2; 32],
    }
}

fn entry() -> ComputationCacheEntry {
    ComputationCacheEntry {
        policy_revision: 19,
        context: ComputationCacheContext {
            token_digest: [2; 32],
            resource: ResourceId::from_u128(1).unwrap(),
            principal: PrincipalId::from_u128(3).unwrap(),
            client: ClientId::from_u128(4).unwrap(),
            credential: CredentialId::from_u128(5).unwrap(),
            principal_epoch: 6,
            scopes: vec!["read".into(), "write".into()],
            capability_ceiling: [CapabilityId::from_u128(7).unwrap()].into(),
        },
        payload: br#"{"bounded":"projection"}"#.to_vec(),
    }
}

#[test]
fn signed_projection_round_trips_only_for_its_versioned_context() {
    let key = key();
    let entry = entry();
    let encoded = encode(&key, &entry, &[9; 32]).unwrap();
    let decoded = decode(&key, &encoded, &[9; 32]).unwrap();
    assert!(decoded.applies_to(19, &entry.context));
    assert_eq!(decoded.payload, entry.payload);
    assert!(decode(&key, &encoded, &[8; 32]).is_err());
    assert!(
        decode(
            &ComputationCacheKey {
                resource: key.resource,
                token_digest: [3; 32],
            },
            &encoded,
            &[9; 32]
        )
        .is_err()
    );
}

#[test]
fn edited_payloads_and_unknown_fields_fail_integrity_validation() {
    let key = key();
    let encoded = encode(&key, &entry(), &[9; 32]).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    value["entry"]["payload"] = format!("{}A", value["entry"]["payload"].as_str().unwrap()).into();
    assert!(decode(&key, &serde_json::to_vec(&value).unwrap(), &[9; 32]).is_err());

    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    value["unexpected"] = true.into();
    assert!(decode(&key, &serde_json::to_vec(&value).unwrap(), &[9; 32]).is_err());
}

#[test]
fn wrong_key_context_and_oversized_payloads_are_not_cached() {
    let key = key();
    let mut mismatched_entry = entry();
    mismatched_entry.context.resource = ResourceId::from_u128(8).unwrap();
    assert!(encode(&key, &mismatched_entry, &[9; 32]).is_err());

    let mut oversized_entry = entry();
    oversized_entry.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
    assert!(encode(&key, &oversized_entry, &[9; 32]).is_err());

    assert!(decode(&key, &vec![b'x'; MAX_VALUE_BYTES as usize + 1], &[9; 32]).is_err());
}

#[test]
fn cache_keys_contain_only_the_resource_and_digest_under_a_versioned_namespace() {
    let key = key();
    let value = wire_key(&key);
    assert!(value.starts_with("darkhorse:authorization:v1:"));
    assert!(value.contains(&Uuid::from_u128(1).to_string()));
    assert!(value.ends_with(&"02".repeat(32)));
}

fn wire_key(key: &ComputationCacheKey) -> String {
    super::key(key)
}
