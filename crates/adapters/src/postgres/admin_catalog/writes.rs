use super::*;
use darkhorse_domain::{
    admin_catalog::{
        PermissionDefinition, capacity, complete_bindings, live_capability, role_binding_safe,
    },
    registration::Label,
};
use std::collections::BTreeSet;
pub(super) struct Plan {
    target: Target,
    effect: Option<Effect>,
}
enum Effect {
    Capability(PermissionDefinition, Option<ApplicationId>),
    Role(Label, Option<ApplicationId>),
    Retire,
    Link(Link),
}
struct Link {
    table: &'static str,
    columns: &'static [&'static str],
    values: Vec<Uuid>,
    insert: bool,
}
pub(super) async fn prepare(
    tx: &mut Tx<'_>,
    change: &Change,
    new_id: Option<NonZeroU128>,
) -> Result<Plan, Error> {
    if change.needs_identifier() != new_id.is_some() {
        return Err(Error::Invalid);
    }
    match change {
        Change::CreateCapability {
            definition,
            application,
        } => {
            optional_application(tx, *application).await?;
            Ok(Plan {
                target: Target::Capability(
                    CapabilityId::from_u128(new_id.ok_or(Error::Invalid)?.get())
                        .map_err(storage)?,
                ),
                effect: Some(Effect::Capability(definition.clone(), *application)),
            })
        }
        Change::CreateRole { name, application } => {
            optional_application(tx, *application).await?;
            Ok(Plan {
                target: Target::Role(
                    RoleId::from_u128(new_id.ok_or(Error::Invalid)?.get()).map_err(storage)?,
                ),
                effect: Some(Effect::Role(name.clone(), *application)),
            })
        }
        Change::RetireCapability(id) => {
            let retired = capability(tx, *id, false).await?;
            Ok(Plan {
                target: Target::Capability(*id),
                effect: (!retired).then_some(Effect::Retire),
            })
        }
        _ => prepare_link(tx, change).await,
    }
}
async fn optional_application(tx: &mut Tx<'_>, app: Option<ApplicationId>) -> Result<(), Error> {
    if let Some(app) = app {
        reads::application_exists(tx, app).await?;
    }
    Ok(())
}
async fn capability(tx: &mut Tx<'_>, id: CapabilityId, grant: bool) -> Result<bool, Error> {
    let retired: bool = sqlx::query_scalar("SELECT retired FROM capabilities WHERE id=$1")
        .bind(uuid(id.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    live_capability(retired, grant)?;
    Ok(retired)
}
async fn prepare_link(tx: &mut Tx<'_>, change: &Change) -> Result<Plan, Error> {
    let (target, link) = link(change)?;
    let view = reads::view(tx, target).await?;
    validate_references(tx, change, &view).await?;
    let exists = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(exists_statement(&link)));
    let mut query = exists;
    for value in &link.values {
        query = query.bind(value);
    }
    let existing = query.fetch_one(&mut **tx).await.map_err(storage)?;
    capacity(
        change,
        view.applications.len(),
        view.capabilities.len(),
        existing,
    )?;
    Ok(Plan {
        target,
        effect: (existing != link.insert).then_some(Effect::Link(link)),
    })
}
async fn validate_references(tx: &mut Tx<'_>, change: &Change, view: &View) -> Result<(), Error> {
    match change {
        Change::CapabilityBinding {
            application,
            capability: id,
            bound,
        } => {
            reads::application_exists(tx, *application).await?;
            capability(tx, *id, *bound).await?;
        }
        Change::RoleBinding {
            application, bound, ..
        } => {
            reads::application_exists(tx, *application).await?;
            if *bound {
                let ids = view
                    .capabilities
                    .iter()
                    .map(|c| uuid(c.id.as_u128()))
                    .collect::<Vec<_>>();
                let missing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM unnest($1::uuid[]) c(id) WHERE NOT EXISTS(SELECT 1 FROM capability_applications b WHERE b.application_id=$2 AND b.capability_id=c.id))").bind(ids).bind(uuid(application.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
                complete_bindings(missing)?;
            }
        }
        Change::RoleCapability {
            capability: id,
            granted,
            ..
        } => {
            capability(tx, *id, *granted).await?;
            if *granted {
                let bound = reads::view(tx, Target::Capability(*id)).await?;
                role_binding_safe(&application_ids(view), &application_ids(&bound))?;
            }
        }
        Change::ResourceCapability {
            application,
            capability: id,
            exposed,
            ..
        } => {
            capability(tx, *id, *exposed).await?;
            if *exposed {
                let bound = reads::view(tx, Target::Capability(*id)).await?;
                role_binding_safe(&[*application].into(), &application_ids(&bound))?;
            }
        }
        Change::ScopeCapability {
            application,
            resource,
            capability: id,
            included,
            ..
        } => {
            capability(tx, *id, *included).await?;
            if *included {
                let resource = reads::view(tx, Target::Resource(*application, *resource)).await?;
                complete_bindings(!resource.capabilities.iter().any(|c| c.id == *id))?;
            }
        }
        _ => return Err(Error::Invalid),
    }
    Ok(())
}
fn application_ids(view: &View) -> BTreeSet<ApplicationId> {
    view.applications.iter().map(|a| a.id).collect()
}
fn link(change: &Change) -> Result<(Target, Link), Error> {
    let (target, table, columns, values, insert) = match change {
        Change::CapabilityBinding {
            application,
            capability,
            bound,
        } => (
            Target::Capability(*capability),
            "capability_applications",
            &["application_id", "capability_id"][..],
            vec![application.as_u128(), capability.as_u128()],
            *bound,
        ),
        Change::RoleBinding {
            application,
            role,
            bound,
        } => (
            Target::Role(*role),
            "role_applications",
            &["application_id", "role_id"][..],
            vec![application.as_u128(), role.as_u128()],
            *bound,
        ),
        Change::RoleCapability {
            role,
            capability,
            granted,
        } => (
            Target::Role(*role),
            "role_capabilities",
            &["role_id", "capability_id"][..],
            vec![role.as_u128(), capability.as_u128()],
            *granted,
        ),
        Change::ResourceCapability {
            application,
            resource,
            capability,
            exposed,
        } => (
            Target::Resource(*application, *resource),
            "resource_capabilities",
            &["application_id", "resource_id", "capability_id"][..],
            vec![
                application.as_u128(),
                resource.as_u128(),
                capability.as_u128(),
            ],
            *exposed,
        ),
        Change::ScopeCapability {
            application,
            resource,
            scope,
            capability,
            included,
        } => (
            Target::Scope(*application, *resource, *scope),
            "scope_capabilities",
            &["application_id", "resource_id", "scope_id", "capability_id"][..],
            vec![
                application.as_u128(),
                resource.as_u128(),
                scope.as_u128(),
                capability.as_u128(),
            ],
            *included,
        ),
        _ => return Err(Error::Invalid),
    };
    Ok((
        target,
        Link {
            table,
            columns,
            values: values.into_iter().map(uuid).collect(),
            insert,
        },
    ))
}
fn predicate(link: &Link) -> String {
    link.columns
        .iter()
        .enumerate()
        .map(|(i, key)| format!("{key}=${}", i + 1))
        .collect::<Vec<_>>()
        .join(" AND ")
}
fn exists_statement(link: &Link) -> String {
    format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE {})",
        link.table,
        predicate(link)
    )
}
fn write_statement(link: &Link) -> String {
    if link.insert {
        format!(
            "INSERT INTO {}({}) VALUES({})",
            link.table,
            link.columns.join(","),
            (1..=link.columns.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        format!("DELETE FROM {} WHERE {}", link.table, predicate(link))
    }
}
pub(super) async fn apply(
    tx: &mut Tx<'_>,
    plan: Plan,
    change: &Change,
    actor: PrincipalId,
    session: SessionId,
    now: u64,
) -> Result<Written, Error> {
    if let Some(effect) = plan.effect {
        persist(tx, plan.target, effect).await?;
        let revision = reads::revision(tx).await?;
        audit(
            tx,
            Audit {
                actor,
                session,
                now,
                revision,
                target: plan.target,
            },
            change,
        )
        .await?;
    }
    Ok(Written {
        target: plan.target,
        policy_revision: reads::revision(tx).await?,
    })
}
async fn persist(tx: &mut Tx<'_>, target: Target, effect: Effect) -> Result<(), Error> {
    match effect {
        Effect::Capability(definition, app) => {
            let Target::Capability(id) = target else {
                return Err(Error::Invalid);
            };
            sqlx::query("INSERT INTO capabilities(id,permission_key,meaning) VALUES($1,$2,$3)")
                .bind(uuid(id.as_u128()))
                .bind(definition.key())
                .bind(definition.meaning())
                .execute(&mut **tx)
                .await
                .map_err(constraint)?;
            if let Some(application) = app {
                let (_, link) = link(&Change::CapabilityBinding {
                    application,
                    capability: id,
                    bound: true,
                })?;
                persist_link(tx, link).await?;
            }
        }
        Effect::Role(name, app) => {
            let Target::Role(id) = target else {
                return Err(Error::Invalid);
            };
            sqlx::query("INSERT INTO roles(id,name) VALUES($1,$2)")
                .bind(uuid(id.as_u128()))
                .bind(name.as_str())
                .execute(&mut **tx)
                .await
                .map_err(constraint)?;
            if let Some(application) = app {
                let (_, link) = link(&Change::RoleBinding {
                    application,
                    role: id,
                    bound: true,
                })?;
                persist_link(tx, link).await?;
            }
        }
        Effect::Retire => {
            let Target::Capability(id) = target else {
                return Err(Error::Invalid);
            };
            sqlx::query("UPDATE capabilities SET retired=true WHERE id=$1")
                .bind(uuid(id.as_u128()))
                .execute(&mut **tx)
                .await
                .map_err(constraint)?;
        }
        Effect::Link(link) => persist_link(tx, link).await?,
    }
    Ok(())
}
async fn persist_link(tx: &mut Tx<'_>, link: Link) -> Result<(), Error> {
    let statement = write_statement(&link);
    let mut query = sqlx::query(sqlx::AssertSqlSafe(statement));
    for value in link.values {
        query = query.bind(value);
    }
    query.execute(&mut **tx).await.map_err(constraint)?;
    Ok(())
}
struct Audit {
    actor: PrincipalId,
    session: SessionId,
    now: u64,
    revision: u64,
    target: Target,
}
async fn audit(tx: &mut Tx<'_>, record: Audit, change: &Change) -> Result<(), Error> {
    let (event, app, related) = audit_values(change);
    sqlx::query("INSERT INTO catalog_admin_audit(actor_id,actor_session_id,policy_revision,event,target_id,application_id,related_id,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(uuid(record.actor.as_u128())).bind(uuid(record.session.as_u128())).bind(record.revision as i64).bind(event).bind(uuid(target_id(record.target))).bind(app.map(|v|uuid(v.as_u128()))).bind(related.map(uuid)).bind(record.now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) fn target_id(target: Target) -> u128 {
    match target {
        Target::Capability(id) => id.as_u128(),
        Target::Role(id) => id.as_u128(),
        Target::Resource(_, id) => id.as_u128(),
        Target::Scope(_, _, id) => id.as_u128(),
    }
}
fn audit_values(change: &Change) -> (&'static str, Option<ApplicationId>, Option<u128>) {
    match change {
        Change::CreateCapability { application, .. } => ("capability_created", *application, None),
        Change::RetireCapability(_) => ("capability_retired", None, None),
        Change::CreateRole { application, .. } => ("role_created", *application, None),
        Change::CapabilityBinding {
            application, bound, ..
        } => (
            if *bound {
                "capability_bound"
            } else {
                "capability_unbound"
            },
            Some(*application),
            None,
        ),
        Change::RoleBinding {
            application, bound, ..
        } => (
            if *bound { "role_bound" } else { "role_unbound" },
            Some(*application),
            None,
        ),
        Change::RoleCapability {
            capability,
            granted,
            ..
        } => (
            if *granted {
                "role_capability_granted"
            } else {
                "role_capability_removed"
            },
            None,
            Some(capability.as_u128()),
        ),
        Change::ResourceCapability {
            application,
            capability,
            exposed,
            ..
        } => (
            if *exposed {
                "resource_capability_exposed"
            } else {
                "resource_capability_removed"
            },
            Some(*application),
            Some(capability.as_u128()),
        ),
        Change::ScopeCapability {
            application,
            capability,
            included,
            ..
        } => (
            if *included {
                "scope_capability_included"
            } else {
                "scope_capability_removed"
            },
            Some(*application),
            Some(capability.as_u128()),
        ),
    }
}
