//! Bounded target projection under the caller's primary security-state fence.
use darkhorse_domain::{AccountStatus, authorization::*, identity::*, tokens::Error};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use std::collections::BTreeSet;
use uuid::Uuid;
mod consent;
pub(super) use consent::{approve, approved};
type Tx<'a> = Transaction<'a, Postgres>;
mod projection;
pub(in crate::postgres) struct Policy {
    catalog: Catalog,
    principal: Principal,
    target: Target,
    client: ClientId,
    scopes: BTreeSet<ScopeId>,
    exposed: CapabilitySet,
}
pub(in crate::postgres) struct StoredGrant {
    pub credential: CredentialId,
    pub resource: ResourceId,
    pub epoch: u64,
    pub ceiling: CapabilitySet,
    pub created: u64,
    pub expires: u64,
}
impl Policy {
    pub(in crate::postgres) fn evaluate(
        &self,
        stored: &StoredGrant,
        now: u64,
    ) -> Result<CapabilitySet, Error> {
        if stored.resource != self.target.resource
            || !darkhorse_domain::tokens::live(stored.created, stored.expires, now)
        {
            return Err(Error::InvalidToken);
        }
        let grant = CredentialGrant {
            credential: stored.credential,
            subject: self.principal.id,
            target: self.target,
            revoked: false,
            principal_epoch: stored.epoch,
            valid_from: stored.created / 1000,
            expires_at: Some(stored.expires / 1000),
            ceiling: stored.ceiling.clone(),
            delegation: Delegation::OAuth {
                client: self.client,
                scopes: self.scopes.clone(),
            },
        };
        effective_capabilities(
            &self.catalog,
            &Evaluation {
                principal: &self.principal,
                credential: &grant,
                target: self.target,
                now: now / 1000,
            },
        )
        .map_err(|_| Error::InvalidToken)
    }
}

pub(super) async fn plan(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    client: ClientId,
    audience: &str,
    scopes: &[String],
    limit: Option<&CapabilitySet>,
) -> Result<IssuancePlan, Error> {
    let policy = load(tx, principal, client, audience, scopes, limit).await?;
    plan_oauth(
        &policy.catalog,
        &policy.principal,
        policy.target,
        client,
        policy.scopes,
        limit.unwrap_or(&policy.exposed),
    )
    .map_err(|_| Error::InvalidGrant)
}
pub(in crate::postgres) async fn load(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    client: ClientId,
    audience: &str,
    scopes: &[String],
    historical: Option<&CapabilitySet>,
) -> Result<Policy, Error> {
    darkhorse_domain::tokens::profile(scopes, Some(audience))?;
    // Acquire the caller's security fence in an earlier SQL statement. Combining
    // it here could retain a snapshot from before a waiting writer committed.
    let row = sqlx::query_as::<_, projection::Projection>(include_str!("projection.sql"))
        .bind(uuid(client.as_u128()))
        .bind(uuid(principal.as_u128()))
        .bind(audience)
        .bind(scopes)
        .bind(historical.map(encoded).unwrap_or_default())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::InvalidGrant)?;
    projection::assemble(&row, principal, client, scopes)
}

pub(in crate::postgres) fn capabilities(values: &[Uuid]) -> Result<CapabilitySet, Error> {
    values
        .iter()
        .map(|v| CapabilityId::from_u128(v.as_u128()).map_err(storage))
        .collect()
}
pub(in crate::postgres) fn encoded(values: &CapabilitySet) -> Vec<Uuid> {
    values.iter().map(|v| uuid(v.as_u128())).collect()
}
fn id(row: &PgRow, field: &str) -> Result<u128, Error> {
    Ok(row.try_get::<Uuid, _>(field).map_err(storage)?.as_u128())
}
fn uuid(id: u128) -> Uuid {
    Uuid::from_u128(id)
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
