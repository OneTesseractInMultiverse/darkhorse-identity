use super::PostgresStore;
use darkhorse_application::oidc::*;
use darkhorse_domain::{
    identity::{ClientId, PrincipalId},
    oidc::*,
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
pub(super) mod authority;
mod maintenance;
mod preauthorization;
pub(super) mod records;
mod writes;
type Tx<'a> = Transaction<'a, Postgres>;
struct Pending {
    request: Request,
    created: u64,
    initial: Option<[u8; 32]>,
    bound: Option<Session>,
    approved: bool,
    client_revision: u64,
    application_revision: u64,
}
pub(super) struct Catalog {
    name: String,
    pub(super) policy: ClientPolicy,
}
impl AuthorizationStore for PostgresStore {
    async fn begin(
        &self,
        request: Request,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
    ) -> Result<Outcome, Error> {
        request.validate()?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let revision = authority::lock(&mut tx).await?;
        let catalog = records::catalog(&mut tx, request.client).await?;
        if !trusted_redirect(&request, &catalog.policy) {
            return Err(Error::InvalidRequest);
        }
        if let Err(error) = authorize_request(&request, &catalog.policy) {
            return Ok(return_to(&request, error));
        }
        let current = authority::session(&mut tx, session).await?;
        let now = authority::now(&mut tx).await?;
        let consented = authority::consent(&mut tx, &request, &catalog.policy, current).await?;
        let interaction = match interaction(&request, now, now, session, None, current, consented) {
            Ok(value) => value,
            Err(error) => return Ok(return_to(&request, error)),
        };
        if interaction == Interaction::Login {
            return Ok(Outcome::Login(Box::new(Preauthorization {
                resource_policy_revision: request.resource.as_ref().map(|_| revision),
                request,
                created_ms: now,
                initial_session: session,
                client_revision: catalog.policy.revision,
                application_revision: catalog.policy.application_revision,
            })));
        }
        writes::capacity(&mut tx, request.client).await?;
        writes::insert(&mut tx, &request, &catalog.policy, handle, session, now).await?;
        writes::bind(&mut tx, handle, current, interaction).await?;
        tx.commit().await.map_err(storage)?;
        Ok(view(catalog.name, &request, interaction))
    }
    async fn continue_login(
        &self,
        pending: Preauthorization,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        decision: Decision,
    ) -> Result<Outcome, Error> {
        preauthorization::continue_login(self, pending, handle, session, decision).await
    }
    async fn resume(
        &self,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        decision: Decision,
    ) -> Result<Outcome, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let outcome = resume(&mut tx, handle, session, decision).await?;
        tx.commit().await.map_err(storage)?;
        Ok(outcome)
    }
}
async fn resume(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    session: Option<[u8; 32]>,
    decision: Decision,
) -> Result<Outcome, Error> {
    let pending = records::pending(tx, handle).await?;
    let catalog = records::catalog(tx, pending.request.client).await?;
    authority::unchanged(&pending, &catalog.policy)?;
    let current = authority::session(tx, session).await?;
    let now = authority::now(tx).await?;
    let consented = pending.approved
        || authority::consent(tx, &pending.request, &catalog.policy, current).await?;
    let mut continued = pending.request.clone();
    continued.prompt = continued_prompt(continued.prompt, pending.approved);
    let stage = interaction(
        &continued,
        pending.created,
        now,
        pending.initial,
        pending.bound,
        current,
        consented,
    )?;
    writes::bind(tx, handle, current, stage).await?;
    let outcome = decide(tx, handle, &pending, &catalog, current, stage, decision).await?;
    Ok(outcome)
}
async fn decide(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    pending: &Pending,
    catalog: &Catalog,
    current: Option<Session>,
    stage: Interaction,
    decision: Decision,
) -> Result<Outcome, Error> {
    match darkhorse_domain::oidc::decision(stage, pending.approved, decision)? {
        Decision::Inspect => Ok(view(catalog.name.clone(), &pending.request, stage)),
        Decision::Deny => {
            writes::finish(tx, handle).await?;
            Ok(return_to(&pending.request, Error::AccessDenied))
        }
        Decision::Approve => {
            writes::approve(
                tx,
                handle,
                &pending.request,
                &catalog.policy,
                current.ok_or(Error::InvalidTransaction)?,
            )
            .await?;
            Ok(view(
                catalog.name.clone(),
                &pending.request,
                Interaction::Ready,
            ))
        }
    }
}
fn view(name: String, request: &Request, interaction: Interaction) -> Outcome {
    Outcome::Pending(View {
        client_name: name,
        scopes: request.scopes.clone(),
        resource: request.resource.clone(),
        interaction,
        ui_locale: request.ui_locale,
    })
}
fn return_to(request: &Request, error: Error) -> Outcome {
    Outcome::Return {
        target: ReturnTo {
            uri: request.redirect.clone(),
            state: request.state.clone(),
        },
        error,
    }
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn integer(n: u64) -> Result<i64, Error> {
    n.try_into().map_err(storage)
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn digest(row: &PgRow, key: &str) -> Result<Option<[u8; 32]>, Error> {
    row.try_get::<Option<Vec<u8>>, _>(key)
        .map_err(storage)?
        .map(|v| v.try_into().map_err(storage))
        .transpose()
}
mod codes;
