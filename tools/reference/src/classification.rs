//! Reviewed boundary labels for every source-derived HTTP route registration.
use crate::inventory::{Entry, Error};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Integration,
    FirstPartyBrowser,
    PublicPresentation,
    Operational,
    StaticAsset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClassifiedEntry {
    #[serde(flatten)]
    pub route: Entry,
    pub surface: Surface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<&'static str>,
    pub rationale: &'static str,
}

const INTEGRATION_RATIONALE: &str =
    "Supported third-party OIDC operation. Its contract is described in the OpenAPI reference.";
const BROWSER_RATIONALE: &str = "First-party browser, account, or administration workflow; this route does not grant third-party API authority.";
const PRESENTATION_RATIONALE: &str = "Public deployment presentation data for the bundled browser portal, not a client integration contract.";
const OPERATIONAL_RATIONALE: &str =
    "Infrastructure health probe only; this response is not evidence of identity or authorization.";
const STATIC_RATIONALE: &str = "Bundled frontend HTML or static asset, not an HTTP API operation.";

pub fn classify(route: Entry) -> Result<ClassifiedEntry, Error> {
    let (surface, operation_id, rationale) = match (
        route.source.as_str(),
        route.method.as_str(),
        route.path.as_str(),
    ) {
        ("crates/adapters/src/provider_http/mod.rs", "GET", "/authorize") => (
            Surface::Integration,
            Some("getAuthorization"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/provider_http/mod.rs", "GET", "/jwks") => {
            (Surface::Integration, Some("getJwks"), INTEGRATION_RATIONALE)
        }
        ("crates/adapters/src/provider_http/mod.rs", "GET", "/api/authorization")
        | ("crates/adapters/src/provider_http/mod.rs", "POST", "/api/authorization/decision") => {
            (Surface::FirstPartyBrowser, None, BROWSER_RATIONALE)
        }
        ("crates/adapters/src/token_http/mod.rs", "GET", "/.well-known/openid-configuration") => (
            Surface::Integration,
            Some("getDiscovery"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/token_http/mod.rs", "POST", "/token") => (
            Surface::Integration,
            Some("postToken"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/token_http/mod.rs", "GET", "/userinfo") => (
            Surface::Integration,
            Some("getUserInfo"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/token_http/mod.rs", "POST", "/introspect") => (
            Surface::Integration,
            Some("postIntrospection"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/token_http/mod.rs", "POST", "/revoke") => (
            Surface::Integration,
            Some("postRevocation"),
            INTEGRATION_RATIONALE,
        ),
        ("crates/adapters/src/localization_http.rs", "GET", "/api/presentation")
        | ("crates/adapters/src/media_http/mod.rs", "GET", "/api/branding")
        | ("crates/adapters/src/media_http/mod.rs", "GET", "/api/branding/{kind}") => {
            (Surface::PublicPresentation, None, PRESENTATION_RATIONALE)
        }
        ("crates/adapters/src/http.rs", "GET", "/health/live")
        | ("crates/adapters/src/readiness.rs", "GET", "/health/ready") => {
            (Surface::Operational, None, OPERATIONAL_RATIONALE)
        }
        ("crates/adapters/src/http.rs", "SERVICE" | "SERVICE_PREFIX", _) => {
            (Surface::StaticAsset, None, STATIC_RATIONALE)
        }
        ("crates/adapters/src/http.rs", _, _) => return Err(Error::Unclassified),
        ("crates/adapters/src/provider_http/mod.rs", _, _)
        | ("crates/adapters/src/token_http/mod.rs", _, _)
        | ("crates/adapters/src/localization_http.rs", _, _)
        | ("crates/adapters/src/readiness.rs", _, _) => return Err(Error::Unclassified),
        (source, _, _) if browser_source(source) => {
            (Surface::FirstPartyBrowser, None, BROWSER_RATIONALE)
        }
        ("crates/adapters/src/media_http/mod.rs", _, _) => {
            (Surface::FirstPartyBrowser, None, BROWSER_RATIONALE)
        }
        _ => return Err(Error::Unclassified),
    };

    Ok(ClassifiedEntry {
        route,
        surface,
        operation_id,
        rationale,
    })
}

pub fn document(routes: Vec<Entry>) -> Result<Vec<u8>, Error> {
    let mut entries = routes
        .into_iter()
        .map(classify)
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.route.cmp(&right.route));
    serde_json::to_vec_pretty(&serde_json::json!({
        "schema": 1,
        "version": env!("CARGO_PKG_VERSION"),
        "scope": "Every source-inventoried HTTP registration has a reviewed caller surface. Only entries with an operationId are supported third-party OIDC contracts. Excluded routes remain subject to their runtime authorization boundaries.",
        "entries": entries,
    }))
    .map_err(|_| Error::Serialization)
}

fn browser_source(source: &str) -> bool {
    matches!(
        source,
        "crates/adapters/src/admin_catalog_http/mod.rs"
            | "crates/adapters/src/admin_directory_http.rs"
            | "crates/adapters/src/authentication_http.rs"
            | "crates/adapters/src/email_verification_http.rs"
            | "crates/adapters/src/invitations_http.rs"
            | "crates/adapters/src/personal_keys_http/mod.rs"
            | "crates/adapters/src/profiles_http/mod.rs"
            | "crates/adapters/src/registration_http/mod.rs"
            | "crates/adapters/src/resource_servers_http/mod.rs"
            | "crates/adapters/src/sessions_http.rs"
    )
}

#[cfg(test)]
#[path = "../tests/unit/classification.rs"]
mod tests;
