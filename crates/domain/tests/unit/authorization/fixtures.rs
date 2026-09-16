use super::*;
use crate::{AccountStatus, identity::*};
use std::collections::BTreeSet;

pub fn app(n: u128) -> ApplicationId {
    ApplicationId::from_u128(n).unwrap()
}
pub fn resource(n: u128) -> ResourceId {
    ResourceId::from_u128(n).unwrap()
}
pub fn cap(n: u128) -> CapabilityId {
    CapabilityId::from_u128(n).unwrap()
}
pub fn role(n: u128) -> RoleId {
    RoleId::from_u128(n).unwrap()
}
pub fn scope(n: u128) -> ScopeId {
    ScopeId::from_u128(n).unwrap()
}
pub fn client(n: u128) -> ClientId {
    ClientId::from_u128(n).unwrap()
}
pub fn principal_id(n: u128) -> PrincipalId {
    PrincipalId::from_u128(n).unwrap()
}
pub fn caps(values: &[u128]) -> CapabilitySet {
    values.iter().map(|&n| cap(n)).collect()
}
pub fn target() -> Target {
    Target {
        application: app(1),
        resource: resource(1),
    }
}

pub fn definitions() -> Definitions {
    Definitions {
        applications: vec![
            Application {
                id: app(1),
                active: true,
            },
            Application {
                id: app(2),
                active: true,
            },
            Application {
                id: app(3),
                active: true,
            },
        ],
        resources: vec![
            Resource {
                id: resource(1),
                application: app(1),
                active: true,
                capabilities: caps(&[1, 2, 3]),
            },
            Resource {
                id: resource(2),
                application: app(2),
                active: true,
                capabilities: caps(&[4]),
            },
        ],
        capabilities: vec![1, 2, 3]
            .into_iter()
            .map(|n| Capability {
                id: cap(n),
                applications: BTreeSet::from([app(1)]),
            })
            .chain([Capability {
                id: cap(4),
                applications: BTreeSet::from([app(2)]),
            }])
            .collect(),
        roles: vec![
            Role {
                id: role(1),
                applications: BTreeSet::from([app(1)]),
                capabilities: caps(&[1, 2]),
            },
            Role {
                id: role(2),
                applications: BTreeSet::from([app(2)]),
                capabilities: caps(&[4]),
            },
        ],
        scopes: vec![
            Scope {
                id: scope(1),
                resource: resource(1),
                capabilities: caps(&[1]),
            },
            Scope {
                id: scope(2),
                resource: resource(1),
                capabilities: caps(&[2]),
            },
            Scope {
                id: scope(3),
                resource: resource(2),
                capabilities: caps(&[4]),
            },
        ],
        clients: vec![Client {
            id: client(1),
            application: app(1),
            active: true,
            resources: BTreeSet::from([resource(1)]),
            scopes: BTreeSet::from([scope(1), scope(2)]),
        }],
    }
}

pub fn principal() -> Principal {
    Principal {
        id: principal_id(1),
        status: AccountStatus::Active,
        credential_epoch: 2,
        assignments: BTreeSet::from([Assignment {
            application: app(1),
            role: role(1),
        }]),
    }
}

pub fn credential() -> CredentialGrant {
    CredentialGrant {
        credential: CredentialId::from_u128(1).unwrap(),
        subject: principal_id(1),
        target: target(),
        revoked: false,
        principal_epoch: 2,
        valid_from: 50,
        expires_at: Some(150),
        ceiling: caps(&[1, 2]),
        delegation: Delegation::OAuth {
            client: client(1),
            scopes: BTreeSet::from([scope(1)]),
        },
    }
}
