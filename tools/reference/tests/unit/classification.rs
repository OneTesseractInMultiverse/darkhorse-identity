use super::*;
use crate::inventory::{Entry, Error};

fn route(source: &str, method: &str, path: &str, handler: &str) -> Entry {
    Entry {
        source: source.into(),
        function: "router".into(),
        path: path.into(),
        method: method.into(),
        handler: handler.into(),
    }
}

#[test]
fn protocol_operations_are_classified_with_stable_operation_ids() {
    let operations = [
        (
            "provider_http/mod.rs",
            "GET",
            "/authorize",
            "getAuthorization",
        ),
        ("provider_http/mod.rs", "GET", "/jwks", "getJwks"),
        (
            "token_http/mod.rs",
            "GET",
            "/.well-known/openid-configuration",
            "getDiscovery",
        ),
        ("token_http/mod.rs", "POST", "/token", "postToken"),
        ("token_http/mod.rs", "GET", "/userinfo", "getUserInfo"),
        (
            "token_http/mod.rs",
            "POST",
            "/introspect",
            "postIntrospection",
        ),
        ("token_http/mod.rs", "POST", "/revoke", "postRevocation"),
    ];

    for (source, method, path, operation_id) in operations {
        let classified = classify(route(
            &format!("crates/adapters/src/{source}"),
            method,
            path,
            "handler",
        ))
        .unwrap();
        assert_eq!(classified.surface, Surface::Integration);
        assert_eq!(classified.operation_id, Some(operation_id));
    }
}

#[test]
fn private_presentation_operational_and_static_surfaces_are_distinguished() {
    let cases = [
        (
            route(
                "crates/adapters/src/admin_catalog_http/mod.rs",
                "POST",
                "/api/admin/catalog",
                "write",
            ),
            Surface::FirstPartyBrowser,
        ),
        (
            route(
                "crates/adapters/src/localization_http.rs",
                "GET",
                "/api/presentation",
                "presentation",
            ),
            Surface::PublicPresentation,
        ),
        (
            route(
                "crates/adapters/src/http.rs",
                "GET",
                "/health/live",
                "liveness",
            ),
            Surface::Operational,
        ),
        (
            route(
                "crates/adapters/src/http.rs",
                "SERVICE",
                "/console/users",
                "static-service",
            ),
            Surface::StaticAsset,
        ),
    ];

    for (entry, surface) in cases {
        assert_eq!(classify(entry).unwrap().surface, surface);
    }
}

#[test]
fn new_protocol_surface_and_unknown_route_sources_require_review() {
    let unreviewed_protocol = route(
        "crates/adapters/src/provider_http/mod.rs",
        "POST",
        "/request-object",
        "request_object",
    );
    let unreviewed_source = route("crates/adapters/src/new_http.rs", "GET", "/api/new", "new");

    assert_eq!(classify(unreviewed_protocol), Err(Error::Unclassified));
    assert_eq!(classify(unreviewed_source), Err(Error::Unclassified));
}
