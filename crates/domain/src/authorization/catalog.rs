use super::model::*;
use crate::identity::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Application,
    Resource,
    Capability,
    Role,
    Scope,
    Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    Duplicate(DefinitionKind),
    UnknownApplication,
    UnknownResource,
    UnknownCapability,
    UnknownScope,
    CapabilityBindingMismatch,
    ScopeResourceMismatch,
}

/// Immutable and fully validated; a new policy revision requires a new catalog.
#[derive(Debug)]
pub struct Catalog {
    pub(super) applications: BTreeMap<ApplicationId, Application>,
    pub(super) resources: BTreeMap<ResourceId, Resource>,
    pub(super) capabilities: BTreeMap<CapabilityId, Capability>,
    pub(super) roles: BTreeMap<RoleId, Role>,
    pub(super) scopes: BTreeMap<ScopeId, Scope>,
    pub(super) clients: BTreeMap<ClientId, Client>,
}

impl Catalog {
    pub fn new(definitions: Definitions) -> Result<Self, CatalogError> {
        let catalog = Self {
            applications: index(
                definitions.applications,
                |v| v.id,
                DefinitionKind::Application,
            )?,
            resources: index(definitions.resources, |v| v.id, DefinitionKind::Resource)?,
            capabilities: index(
                definitions.capabilities,
                |v| v.id,
                DefinitionKind::Capability,
            )?,
            roles: index(definitions.roles, |v| v.id, DefinitionKind::Role)?,
            scopes: index(definitions.scopes, |v| v.id, DefinitionKind::Scope)?,
            clients: index(definitions.clients, |v| v.id, DefinitionKind::Client)?,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), CatalogError> {
        for capability in self.capabilities.values() {
            for &application in &capability.applications {
                self.application_exists(application)?;
            }
        }
        for resource in self.resources.values() {
            self.validate_resource(resource)?;
        }
        for role in self.roles.values() {
            self.validate_role(role)?;
        }
        for scope in self.scopes.values() {
            self.validate_scope(scope)?;
        }
        for client in self.clients.values() {
            self.validate_client(client)?;
        }
        Ok(())
    }

    fn application_exists(&self, id: ApplicationId) -> Result<(), CatalogError> {
        self.applications
            .get(&id)
            .map(|_| ())
            .ok_or(CatalogError::UnknownApplication)
    }

    fn capability(&self, id: CapabilityId) -> Result<&Capability, CatalogError> {
        self.capabilities
            .get(&id)
            .ok_or(CatalogError::UnknownCapability)
    }

    fn validate_resource(&self, resource: &Resource) -> Result<(), CatalogError> {
        self.application_exists(resource.application)?;
        for &id in &resource.capabilities {
            if !self
                .capability(id)?
                .applications
                .contains(&resource.application)
            {
                return Err(CatalogError::CapabilityBindingMismatch);
            }
        }
        Ok(())
    }

    fn validate_role(&self, role: &Role) -> Result<(), CatalogError> {
        for &application in &role.applications {
            self.application_exists(application)?;
        }
        for &id in &role.capabilities {
            if !role
                .applications
                .is_subset(&self.capability(id)?.applications)
            {
                return Err(CatalogError::CapabilityBindingMismatch);
            }
        }
        Ok(())
    }

    fn validate_scope(&self, scope: &Scope) -> Result<(), CatalogError> {
        let resource = self
            .resources
            .get(&scope.resource)
            .ok_or(CatalogError::UnknownResource)?;
        for &id in &scope.capabilities {
            self.capability(id)?;
        }
        if !scope.capabilities.is_subset(&resource.capabilities) {
            return Err(CatalogError::CapabilityBindingMismatch);
        }
        Ok(())
    }

    fn validate_client(&self, client: &Client) -> Result<(), CatalogError> {
        self.application_exists(client.application)?;
        for id in &client.resources {
            if !self.resources.contains_key(id) {
                return Err(CatalogError::UnknownResource);
            }
        }
        for id in &client.scopes {
            let scope = self.scopes.get(id).ok_or(CatalogError::UnknownScope)?;
            if !client.resources.contains(&scope.resource) {
                return Err(CatalogError::ScopeResourceMismatch);
            }
        }
        Ok(())
    }
}

fn index<K: Ord, V>(
    values: Vec<V>,
    key: impl Fn(&V) -> K,
    kind: DefinitionKind,
) -> Result<BTreeMap<K, V>, CatalogError> {
    let mut result = BTreeMap::new();
    for value in values {
        if result.insert(key(&value), value).is_some() {
            return Err(CatalogError::Duplicate(kind));
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/unit/authorization/catalog.rs"]
mod tests;
