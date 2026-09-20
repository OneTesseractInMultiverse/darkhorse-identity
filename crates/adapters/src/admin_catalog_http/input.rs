use super::*;
use darkhorse_domain::admin_catalog::{Change, PermissionDefinition};
use darkhorse_domain::registration::Label;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mutation {
    #[serde(deserialize_with = "crate::registration_http::input::revision")]
    pub policy_revision: u64,
    pub change: Input,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Input {
    CreateCapability {
        key: String,
        meaning: String,
        application_id: Option<String>,
    },
    RetireCapability {
        capability_id: String,
    },
    CreateRole {
        name: String,
        application_id: Option<String>,
    },
    CapabilityBinding {
        application_id: String,
        capability_id: String,
        bound: bool,
    },
    RoleBinding {
        application_id: String,
        role_id: String,
        bound: bool,
    },
    RoleCapability {
        role_id: String,
        capability_id: String,
        granted: bool,
    },
    ResourceCapability {
        application_id: String,
        resource_id: String,
        capability_id: String,
        exposed: bool,
    },
    ScopeCapability {
        application_id: String,
        resource_id: String,
        scope_id: String,
        capability_id: String,
        included: bool,
    },
}
impl Input {
    pub(super) fn change(self) -> Result<Change, Error> {
        Ok(match self {
            Self::CreateCapability {
                key,
                meaning,
                application_id,
            } => Change::CreateCapability {
                definition: PermissionDefinition::new(&key, &meaning)?,
                application: application_id
                    .map(|s| id(&s, ApplicationId::from_u128))
                    .transpose()?,
            },
            Self::RetireCapability { capability_id } => {
                Change::RetireCapability(id(&capability_id, CapabilityId::from_u128)?)
            }
            Self::CreateRole {
                name,
                application_id,
            } => Change::CreateRole {
                name: Label::new(&name)?,
                application: application_id
                    .map(|s| id(&s, ApplicationId::from_u128))
                    .transpose()?,
            },
            Self::CapabilityBinding {
                application_id,
                capability_id,
                bound,
            } => Change::CapabilityBinding {
                application: id(&application_id, ApplicationId::from_u128)?,
                capability: id(&capability_id, CapabilityId::from_u128)?,
                bound,
            },
            Self::RoleBinding {
                application_id,
                role_id,
                bound,
            } => Change::RoleBinding {
                application: id(&application_id, ApplicationId::from_u128)?,
                role: id(&role_id, RoleId::from_u128)?,
                bound,
            },
            Self::RoleCapability {
                role_id,
                capability_id,
                granted,
            } => Change::RoleCapability {
                role: id(&role_id, RoleId::from_u128)?,
                capability: id(&capability_id, CapabilityId::from_u128)?,
                granted,
            },
            Self::ResourceCapability {
                application_id,
                resource_id,
                capability_id,
                exposed,
            } => Change::ResourceCapability {
                application: id(&application_id, ApplicationId::from_u128)?,
                resource: id(&resource_id, ResourceId::from_u128)?,
                capability: id(&capability_id, CapabilityId::from_u128)?,
                exposed,
            },
            Self::ScopeCapability {
                application_id,
                resource_id,
                scope_id,
                capability_id,
                included,
            } => Change::ScopeCapability {
                application: id(&application_id, ApplicationId::from_u128)?,
                resource: id(&resource_id, ResourceId::from_u128)?,
                scope: id(&scope_id, ScopeId::from_u128)?,
                capability: id(&capability_id, CapabilityId::from_u128)?,
                included,
            },
        })
    }
}
fn parameters(raw: Option<&str>) -> Result<BTreeMap<String, String>, Error> {
    let raw = raw.unwrap_or("");
    if raw.len() > 2048 {
        return Err(Error::Invalid);
    }
    let mut result = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if result
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(Error::Invalid);
        }
    }
    Ok(result)
}
pub(super) fn list(
    kind: &str,
    raw: Option<&str>,
) -> Result<(List, darkhorse_domain::admin_catalog::Query), Error> {
    let mut parameters = parameters(raw)?;
    let app = parameters
        .remove("application_id")
        .map(|s| id(&s, ApplicationId::from_u128))
        .transpose()?;
    let target = match (kind, app) {
        ("applications", None) => List::Applications,
        ("clients", Some(app)) => List::Clients(app),
        ("resources", Some(app)) => List::Resources(app),
        ("scopes", Some(app)) => List::Scopes(app),
        ("capabilities", app) => List::Capabilities(app),
        ("roles", app) => List::Roles(app),
        _ => return Err(Error::Invalid),
    };
    let raw = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(parameters)
        .finish();
    let query = crate::directory_query::parse_catalog(&raw).map_err(|_| Error::Invalid)?;
    Ok((target, query))
}
pub(super) fn target(kind: &str, value: &str, raw: Option<&str>) -> Result<Target, Error> {
    let mut p = parameters(raw)?;
    let app = p
        .remove("application_id")
        .map(|s| id(&s, ApplicationId::from_u128))
        .transpose()?;
    let res = p
        .remove("resource_id")
        .map(|s| id(&s, ResourceId::from_u128))
        .transpose()?;
    if !p.is_empty() {
        return Err(Error::Invalid);
    }
    match (kind, app, res) {
        ("capabilities", None, None) => Ok(Target::Capability(id(value, CapabilityId::from_u128)?)),
        ("roles", None, None) => Ok(Target::Role(id(value, RoleId::from_u128)?)),
        ("resources", Some(app), None) => {
            Ok(Target::Resource(app, id(value, ResourceId::from_u128)?))
        }
        ("scopes", Some(app), Some(res)) => {
            Ok(Target::Scope(app, res, id(value, ScopeId::from_u128)?))
        }
        _ => Err(Error::Invalid),
    }
}
