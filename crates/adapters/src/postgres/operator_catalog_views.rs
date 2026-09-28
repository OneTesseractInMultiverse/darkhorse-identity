use super::{PostgresStore, admin_catalog, operator_accounts, sessions};
use darkhorse_application::{
    admin_catalog::{CapabilitySummary, View},
    operator_accounts::{CandidateAt, Store, Verified},
};
use darkhorse_domain::{
    identity::OperationId,
    operator_accounts::{Error, needs_current_authority},
    operator_catalog::{Definitions, ViewRequest, ViewTarget},
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RELATED_RECORD_LIMIT: usize = 25;

pub struct CatalogViews<'a>(&'a PostgresStore);
impl PostgresStore {
    pub fn operator_catalog_views(&self) -> CatalogViews<'_> {
        CatalogViews(self)
    }
}
impl Store for CatalogViews<'_> {
    type Request = ViewRequest;
    type Outcome = View;

    async fn candidate(&self, email: &str) -> Result<Option<CandidateAt>, Error> {
        Store::candidate(self.0, email).await
    }

    async fn denied(&self, id: OperationId, request: &ViewRequest) -> Result<(), Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        audit(&mut tx, id, *request, None, &Err(Error::Denied)).await?;
        tx.commit().await.map_err(|_| Error::Uncertain)
    }

    async fn execute(&self, proof: Verified<ViewRequest>) -> Result<View, Error> {
        let mut tx = self.0.pool.begin().await.map_err(storage)?;
        sessions::lock(&mut tx, false).await.map_err(storage)?;
        let result = read(&mut tx, &proof).await;
        if matches!(result, Err(Error::Unavailable)) {
            return result;
        }
        audit(
            &mut tx,
            proof.id(),
            *proof.request(),
            Some(proof.candidate()),
            &result,
        )
        .await?;
        if needs_current_authority(&result) {
            operator_accounts::authority(&mut tx, &proof).await?;
        }
        tx.commit().await.map_err(|_| Error::Uncertain)?;
        result
    }
}

async fn read(
    tx: &mut Transaction<'_, Postgres>,
    proof: &Verified<ViewRequest>,
) -> Result<View, Error> {
    operator_accounts::authority(tx, proof).await?;
    let mut view = admin_catalog::view_current(tx, catalog_target(proof.request().target()))
        .await
        .map_err(registration_error)?;
    let target = proof.request().target();
    let allowed_capabilities = match target {
        ViewTarget::Role {
            selection: Definitions::Application(application),
            ..
        } => {
            let ids = capability_ids(&view.capabilities);
            Some(query_bound_capabilities(tx, application, &ids).await?)
        }
        _ => None,
    };
    scope_view(target, &mut view, allowed_capabilities.as_deref())?;
    if view.applications.len() > RELATED_RECORD_LIMIT
        || view.capabilities.len() > RELATED_RECORD_LIMIT
    {
        return Err(Error::Unavailable);
    }
    operator_accounts::authority(tx, proof).await?;
    Ok(view)
}

fn scope_view(
    target: ViewTarget,
    view: &mut View,
    allowed_capabilities: Option<&[Uuid]>,
) -> Result<(), Error> {
    let application = match target {
        ViewTarget::Capability { selection, .. } | ViewTarget::Role { selection, .. } => {
            selection.application()
        }
        ViewTarget::Resource { .. } | ViewTarget::Scope { .. } => None,
    };
    let Some(application) = application else {
        return Ok(());
    };
    if !view
        .applications
        .iter()
        .any(|record| record.id == application)
    {
        return Err(Error::NotFound);
    }
    view.applications.retain(|record| record.id == application);
    if matches!(target, ViewTarget::Role { .. }) {
        let allowed = allowed_capabilities.ok_or(Error::Unavailable)?;
        view.capabilities
            .retain(|capability| allowed.contains(&Uuid::from_u128(capability.id.as_u128())));
    }
    Ok(())
}

fn capability_ids(capabilities: &[CapabilitySummary]) -> Vec<Uuid> {
    capabilities
        .iter()
        .map(|capability| Uuid::from_u128(capability.id.as_u128()))
        .collect()
}

async fn query_bound_capabilities(
    tx: &mut Transaction<'_, Postgres>,
    application: darkhorse_domain::identity::ApplicationId,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let allowed = sqlx::query_scalar::<_, Uuid>(
        "SELECT capability_id FROM capability_applications WHERE application_id=$1 AND capability_id=ANY($2::uuid[])",
    )
    .bind(Uuid::from_u128(application.as_u128()))
    .bind(ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(allowed)
}

fn catalog_target(target: ViewTarget) -> darkhorse_application::admin_catalog::Target {
    use darkhorse_application::admin_catalog::Target;
    match target {
        ViewTarget::Capability { id, .. } => Target::Capability(id),
        ViewTarget::Role { id, .. } => Target::Role(id),
        ViewTarget::Resource { application, id } => Target::Resource(application, id),
        ViewTarget::Scope {
            application,
            resource,
            id,
        } => Target::Scope(application, resource, id),
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: OperationId,
    request: ViewRequest,
    actor: Option<&CandidateAt>,
    result: &Result<View, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let (command, selection, object, resource) = audit_target(request.target());
    let inserted = sqlx::query("INSERT INTO operator_access_detail_audit(operation_id,command,application_id,target_id,resource_id,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(Uuid::from_u128(id.as_u128()))
        .bind(command)
        .bind(selection.map(|value| Uuid::from_u128(value.as_u128())))
        .bind(Uuid::from_u128(object))
        .bind(resource.map(|value| Uuid::from_u128(value.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.principal.as_u128())))
        .bind(actor.map(|a| Uuid::from_u128(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(match result { Ok(_) => "read", Err(Error::NotFound) => "not_found", Err(_) => "denied" })
        .bind(i64::try_from(now).map_err(storage)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}

fn audit_target(
    target: ViewTarget,
) -> (
    &'static str,
    Option<darkhorse_domain::identity::ApplicationId>,
    u128,
    Option<darkhorse_domain::identity::ResourceId>,
) {
    match target {
        ViewTarget::Capability { id, selection } => (
            "capability.show",
            selection.application(),
            id.as_u128(),
            None,
        ),
        ViewTarget::Role { id, selection } => {
            ("role.show", selection.application(), id.as_u128(), None)
        }
        ViewTarget::Resource { application, id } => {
            ("resource.show", Some(application), id.as_u128(), None)
        }
        ViewTarget::Scope {
            application,
            resource,
            id,
        } => (
            "scope.show",
            Some(application),
            id.as_u128(),
            Some(resource),
        ),
    }
}

fn registration_error(error: darkhorse_domain::registration::RegistrationError) -> Error {
    match error {
        darkhorse_domain::registration::RegistrationError::NotFound => Error::NotFound,
        _ => Error::Unavailable,
    }
}

fn storage<T>(_: T) -> Error {
    Error::Unavailable
}

#[cfg(test)]
#[path = "../../tests/unit/postgres/operator_catalog_views.rs"]
mod tests;
