//! Authenticated administration. Implementations recheck authority in each transaction.
use darkhorse_domain::{
    AccountStatus,
    admin_directory::{Change, Error, Query},
    identity::{ApplicationId, PrincipalId, RoleId},
};
use std::future::Future;
#[derive(Debug, Clone)]
pub struct User {
    pub id: PrincipalId,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub status: AccountStatus,
    pub administrator: bool,
    pub email_verified: bool,
    pub revision: u64,
}
pub struct Page {
    pub actor: PrincipalId,
    pub items: Vec<User>,
    pub next: Option<PrincipalId>,
}
pub struct ApplicationSummary {
    pub id: ApplicationId,
    pub name: String,
    pub active: bool,
}
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub assigned: bool,
}
pub struct Access {
    pub user: User,
    pub policy_revision: u64,
    pub applications: Vec<ApplicationSummary>,
    pub selected: Option<ApplicationId>,
    pub roles: Vec<Role>,
}
pub trait AdminDirectory: Send + Sync {
    fn users(
        &self,
        actor: [u8; 32],
        query: Query,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    fn user(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
    ) -> impl Future<Output = Result<User, Error>> + Send;
    fn access(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        application: Option<ApplicationId>,
    ) -> impl Future<Output = Result<Access, Error>> + Send;
    fn update(
        &self,
        actor: [u8; 32],
        id: PrincipalId,
        revision: u64,
        change: Change,
    ) -> impl Future<Output = Result<User, Error>> + Send;
}
