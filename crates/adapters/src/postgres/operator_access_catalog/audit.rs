use super::*;
use crate::postgres::sessions;
use uuid::Uuid;

pub(super) async fn insert(
    tx: &mut Tx<'_>,
    operation: OperationId,
    request: &MutationRequest,
    actor: Option<&CandidateAt>,
    outcome: &Result<Written, Error>,
) -> Result<(), Error> {
    let now = sessions::now(tx).await.map_err(storage)?;
    let values = values(request.change(), outcome);
    let result = result(outcome, request.policy_revision())?;
    let inserted = sqlx::query("INSERT INTO operator_access_catalog_audit(operation_id,command,expected_revision,resulting_revision,target_id,application_id,related_id,requested_state,principal_expected_revision,principal_resulting_revision,reason,actor_id,actor_credential_id,actor_epoch,authentication_observed_ms,result,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
        .bind(uuid(operation.as_u128()))
        .bind(command(request.change()))
        .bind(i64::try_from(request.policy_revision()).map_err(storage)?)
        .bind(outcome.as_ref().ok().map(|written| i64::try_from(written.policy_revision)).transpose().map_err(storage)?)
        .bind(values.target)
        .bind(values.application)
        .bind(values.related)
        .bind(values.requested_state)
        .bind(values.principal_expected_revision)
        .bind(values.principal_resulting_revision)
        .bind(request.reason())
        .bind(actor.map(|a| uuid(a.credential.principal.as_u128())))
        .bind(actor.map(|a| uuid(a.credential.credential.as_u128())))
        .bind(actor.map(|a| i64::try_from(a.credential.epoch)).transpose().map_err(storage)?)
        .bind(actor.map(|a| i64::try_from(a.observed_ms)).transpose().map_err(storage)?)
        .bind(result)
        .bind(i64::try_from(now).map_err(storage)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    if inserted.rows_affected() != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}

struct Values {
    target: Option<Uuid>,
    application: Option<Uuid>,
    related: Option<Uuid>,
    requested_state: Option<bool>,
    principal_expected_revision: Option<i64>,
    principal_resulting_revision: Option<i64>,
}

fn values(change: &Change, outcome: &Result<Written, Error>) -> Values {
    let mut values = match change {
        Change::CreateCapability { application, .. } => Values {
            target: None,
            application: application.map(|id| uuid(id.as_u128())),
            related: None,
            requested_state: None,
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::RetireCapability(id) => Values {
            target: Some(uuid(id.as_u128())),
            application: None,
            related: None,
            requested_state: Some(true),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::CreateRole { application, .. } => Values {
            target: None,
            application: application.map(|id| uuid(id.as_u128())),
            related: None,
            requested_state: None,
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::CapabilityBinding {
            application,
            capability,
            bound,
        } => Values {
            target: Some(uuid(capability.as_u128())),
            application: Some(uuid(application.as_u128())),
            related: None,
            requested_state: Some(*bound),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::RoleBinding {
            application,
            role,
            bound,
        } => Values {
            target: Some(uuid(role.as_u128())),
            application: Some(uuid(application.as_u128())),
            related: None,
            requested_state: Some(*bound),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::RoleCapability {
            role,
            capability,
            granted,
        } => Values {
            target: Some(uuid(role.as_u128())),
            application: None,
            related: Some(uuid(capability.as_u128())),
            requested_state: Some(*granted),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::ResourceCapability {
            application,
            resource,
            capability,
            exposed,
        } => Values {
            target: Some(uuid(resource.as_u128())),
            application: Some(uuid(application.as_u128())),
            related: Some(uuid(capability.as_u128())),
            requested_state: Some(*exposed),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::ScopeCapability {
            application,
            scope,
            capability,
            included,
            ..
        } => Values {
            target: Some(uuid(scope.as_u128())),
            application: Some(uuid(application.as_u128())),
            related: Some(uuid(capability.as_u128())),
            requested_state: Some(*included),
            principal_expected_revision: None,
            principal_resulting_revision: None,
        },
        Change::PrincipalRole {
            principal,
            application,
            role,
            assigned,
            principal_revision,
        } => Values {
            target: Some(uuid(principal.as_u128())),
            application: Some(uuid(application.as_u128())),
            related: Some(uuid(role.as_u128())),
            requested_state: Some(*assigned),
            principal_expected_revision: Some(*principal_revision as i64),
            principal_resulting_revision: outcome
                .as_ref()
                .ok()
                .and_then(|written| written.principal_revision)
                .and_then(|revision| i64::try_from(revision).ok()),
        },
    };
    if let Ok(written) = outcome {
        values.target = Some(uuid(target_id(written.target)));
    }
    values
}

fn target_id(target: Target) -> u128 {
    match target {
        Target::Capability(id) => id.as_u128(),
        Target::Role(id) => id.as_u128(),
        Target::Resource(_, id) => id.as_u128(),
        Target::Scope(_, _, id) => id.as_u128(),
        Target::PrincipalRole(principal, _, _) => principal.as_u128(),
    }
}

fn command(change: &Change) -> &'static str {
    match change {
        Change::PrincipalRole { .. } => "principal.role",
        Change::CreateCapability { .. } => "capability.create",
        Change::RetireCapability(_) => "capability.retire",
        Change::CreateRole { .. } => "role.create",
        Change::CapabilityBinding { .. } => "capability.binding",
        Change::RoleBinding { .. } => "role.binding",
        Change::RoleCapability { .. } => "role.capability",
        Change::ResourceCapability { .. } => "resource.capability",
        Change::ScopeCapability { .. } => "scope.capability",
    }
}

fn result(outcome: &Result<Written, Error>, expected_revision: u64) -> Result<&'static str, Error> {
    match outcome {
        Ok(written) if written.policy_revision == expected_revision => Ok("unchanged"),
        Ok(written) if written.policy_revision > expected_revision => Ok("changed"),
        Ok(_) => Err(Error::Unavailable),
        Err(Error::Denied) => Ok("denied"),
        Err(Error::Invalid) => Ok("invalid"),
        Err(Error::NotFound) => Ok("not_found"),
        Err(Error::Conflict) => Ok("conflict"),
        Err(Error::PolicyRejected) => Ok("policy_rejected"),
        Err(Error::Limited { .. } | Error::Unavailable | Error::Uncertain) => {
            Err(Error::Unavailable)
        }
    }
}

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn storage<T>(_: T) -> Error {
    Error::Unavailable
}

#[cfg(test)]
#[path = "../../../tests/unit/postgres/operator_access_catalog/audit.rs"]
mod tests;
