//! Bounded catalog reads available to an authenticated platform administrator.
use crate::{
    admin_catalog::Query,
    identity::{ApplicationId, CapabilityId, ResourceId, RoleId, ScopeId},
    operator_accounts::Error,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Applications,
    Clients(ApplicationId),
    Resources(ApplicationId),
    Scopes(ApplicationId),
    Capabilities(Definitions),
    Roles(Definitions),
}
/// All definitions includes bound and unbound definitions; it is not a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Definitions {
    Application(ApplicationId),
    All,
}
impl Definitions {
    pub fn application(self) -> Option<ApplicationId> {
        match self {
            Self::Application(id) => Some(id),
            Self::All => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    target: Target,
    query: Query,
}

/// A bounded detail target. Definitions always require an explicit application
/// view or an explicit all-definitions view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTarget {
    Capability {
        id: CapabilityId,
        selection: Definitions,
    },
    Role {
        id: RoleId,
        selection: Definitions,
    },
    Resource {
        application: ApplicationId,
        id: ResourceId,
    },
    Scope {
        application: ApplicationId,
        resource: ResourceId,
        id: ScopeId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewRequest {
    target: ViewTarget,
}
impl ViewRequest {
    pub fn new(target: ViewTarget) -> Self {
        Self { target }
    }
    pub fn target(self) -> ViewTarget {
        self.target
    }
}
impl Request {
    pub fn new(target: Target, query: Query) -> Result<Self, Error> {
        query.validate().map_err(|_| Error::Invalid)?;
        if query.limit > 25
            || (query.active.is_some()
                && matches!(
                    target,
                    Target::Resources(_) | Target::Scopes(_) | Target::Roles(_)
                ))
        {
            return Err(Error::Invalid);
        }
        Ok(Self { target, query })
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn query(&self) -> &Query {
        &self.query
    }
}
#[cfg(test)]
#[path = "../tests/unit/operator_catalog.rs"]
mod tests;
