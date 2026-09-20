//! Catalog input policies independent of HTTP and persistence.
use crate::{
    identity::*,
    registration::{Label, RegistrationError as Error},
};
use std::{collections::BTreeSet, num::NonZeroU128};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub search: String,
    pub active: Option<bool>,
    pub after: Option<NonZeroU128>,
    pub limit: u16,
}
impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=100).contains(&self.limit)
            || self.search.chars().count() > 100
            || self.search.trim() != self.search
            || self.search.chars().any(char::is_control)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDefinition {
    key: String,
    meaning: String,
}
impl PermissionDefinition {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn meaning(&self) -> &str {
        &self.meaning
    }
    pub fn new(key: &str, meaning: &str) -> Result<Self, Error> {
        let meaning = meaning.trim();
        if key.is_empty()
            || key.len() > 200
            || !key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._:-".contains(&b))
            || !key.as_bytes()[0].is_ascii_alphanumeric()
            || meaning.is_empty()
            || meaning.chars().count() > 1000
            || meaning.chars().any(char::is_control)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            key: key.into(),
            meaning: meaning.into(),
        })
    }
}
#[derive(Debug, Clone)]
pub enum Change {
    CreateCapability {
        definition: PermissionDefinition,
        application: Option<ApplicationId>,
    },
    RetireCapability(CapabilityId),
    CreateRole {
        name: Label,
        application: Option<ApplicationId>,
    },
    CapabilityBinding {
        application: ApplicationId,
        capability: CapabilityId,
        bound: bool,
    },
    RoleBinding {
        application: ApplicationId,
        role: RoleId,
        bound: bool,
    },
    RoleCapability {
        role: RoleId,
        capability: CapabilityId,
        granted: bool,
    },
    ResourceCapability {
        application: ApplicationId,
        resource: ResourceId,
        capability: CapabilityId,
        exposed: bool,
    },
    ScopeCapability {
        application: ApplicationId,
        resource: ResourceId,
        scope: ScopeId,
        capability: CapabilityId,
        included: bool,
    },
}
impl Change {
    pub fn needs_identifier(&self) -> bool {
        matches!(
            self,
            Self::CreateCapability { .. } | Self::CreateRole { .. }
        )
    }
}
pub fn role_binding_safe(
    role_apps: &BTreeSet<ApplicationId>,
    capability_apps: &BTreeSet<ApplicationId>,
) -> Result<(), Error> {
    if !role_apps.is_subset(capability_apps) {
        return Err(Error::Invalid);
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unit/admin_catalog.rs"]
mod tests;

pub fn live_capability(retired: bool, grant: bool) -> Result<(), Error> {
    if retired && grant {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
pub fn complete_bindings(missing: bool) -> Result<(), Error> {
    if missing { Err(Error::Invalid) } else { Ok(()) }
}
pub fn capacity(
    change: &Change,
    applications: usize,
    capabilities: usize,
    existing: bool,
) -> Result<(), Error> {
    let (adding, count, limit) = match change {
        Change::CapabilityBinding { bound, .. } | Change::RoleBinding { bound, .. } => {
            (*bound, applications, 1000)
        }
        Change::RoleCapability { granted, .. } => (*granted, capabilities, 256),
        Change::ResourceCapability { exposed, .. } => (*exposed, capabilities, 256),
        Change::ScopeCapability { included, .. } => (*included, capabilities, 256),
        _ => return Ok(()),
    };
    if adding && !existing && count >= limit {
        return Err(Error::Invalid);
    }
    Ok(())
}
