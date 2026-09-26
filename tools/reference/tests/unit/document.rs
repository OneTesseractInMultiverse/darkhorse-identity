use super::*;

#[test]
fn document_serializes_a_versioned_deterministic_inventory() {
    let entries = vec![Entry {
        source: "src/router.rs".into(),
        function: "router".into(),
        path: "/token".into(),
        method: "POST".into(),
        handler: "redeem".into(),
    }];

    let serialized = document(entries.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&serialized).unwrap();

    assert_eq!(parsed["schema"], 1);
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["entries"][0]["path"], "/token");
    assert_eq!(parsed["entries"][0]["handler"], "redeem");
    assert_eq!(serialized, document(entries).unwrap());
}
