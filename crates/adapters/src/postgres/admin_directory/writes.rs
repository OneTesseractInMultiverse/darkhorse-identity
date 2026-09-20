use super::*;
use darkhorse_application::directory::AccountRecord;
use darkhorse_domain::admin_directory::Names;
pub(super) async fn apply(
    tx: &mut Tx<'_>,
    current: &AccountRecord,
    change: &Change,
) -> Result<bool, Error> {
    match change {
        Change::Status(status) => directory::change_locked(
            tx,
            current.id,
            current.revision,
            AccountAction::SetStatus(*status),
        )
        .await
        .map(|r| r.is_some())
        .map_err(directory_error),
        Change::Names(names) => rename(tx, current, names).await,
        Change::Role {
            application,
            role,
            assigned,
            policy_revision,
        } => {
            assign(
                tx,
                current,
                *application,
                *role,
                *assigned,
                *policy_revision,
            )
            .await
        }
    }
}
async fn rename(tx: &mut Tx<'_>, current: &AccountRecord, names: &Names) -> Result<bool, Error> {
    let names = Names::new(&names.first, &names.last)?;
    if current.profile.first_name() == names.first && current.profile.last_name() == names.last {
        return Ok(false);
    }
    sqlx::query("UPDATE principals SET first_name=$2,last_name=$3,revision=$4 WHERE id=$1")
        .bind(uuid(current.id.as_u128()))
        .bind(names.first)
        .bind(names.last)
        .bind(revision(current.revision, current.revision)? as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(true)
}
async fn assign(
    tx: &mut Tx<'_>,
    current: &AccountRecord,
    app: ApplicationId,
    role: RoleId,
    assigned: bool,
    expected: u64,
) -> Result<bool, Error> {
    let policy_revision = reads::policy_revision(tx).await?;
    let active:bool=sqlx::query_scalar("SELECT a.active FROM applications a JOIN role_applications r ON r.application_id=a.id WHERE a.id=$1 AND r.role_id=$2")
        .bind(uuid(app.as_u128())).bind(uuid(role.as_u128())).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::Invalid)?;
    let existing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM principal_roles WHERE principal_id=$1 AND application_id=$2 AND role_id=$3)")
        .bind(uuid(current.id.as_u128())).bind(uuid(app.as_u128())).bind(uuid(role.as_u128())).fetch_one(&mut **tx).await.map_err(storage)?;
    if !darkhorse_domain::admin_directory::role_change(
        policy_revision,
        expected,
        active,
        existing,
        assigned,
    )? {
        return Ok(false);
    }
    let statement = if assigned {
        "INSERT INTO principal_roles(principal_id,application_id,role_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM principal_roles WHERE principal_id=$1 AND application_id=$2 AND role_id=$3"
    };
    let affected = sqlx::query(statement)
        .bind(uuid(current.id.as_u128()))
        .bind(uuid(app.as_u128()))
        .bind(uuid(role.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?
        .rows_affected();
    if affected == 0 {
        return Ok(false);
    }
    sqlx::query("UPDATE principals SET revision=$2 WHERE id=$1")
        .bind(uuid(current.id.as_u128()))
        .bind(revision(current.revision, current.revision)? as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(true)
}
pub(super) struct Audit {
    pub actor: PrincipalId,
    pub session: SessionId,
    pub target: PrincipalId,
    pub revision: u64,
    pub now: u64,
}
pub(super) async fn audit(tx: &mut Tx<'_>, record: Audit, change: &Change) -> Result<(), Error> {
    let Audit {
        actor,
        session,
        target,
        revision,
        now,
    } = record;
    let (event, app, role) = audit_values(change);
    sqlx::query("INSERT INTO directory_admin_audit(actor_id,actor_session_id,target_id,target_revision,event,application_id,role_id,occurred_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(uuid(actor.as_u128())).bind(uuid(session.as_u128())).bind(uuid(target.as_u128())).bind(revision as i64).bind(event).bind(app).bind(role).bind(now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
fn audit_values(change: &Change) -> (&'static str, Option<Uuid>, Option<Uuid>) {
    match change {
        Change::Names(_) => ("names_changed", None, None),
        Change::Status(AccountStatus::Active) => ("reactivated", None, None),
        Change::Status(AccountStatus::Inactive) => ("deactivated", None, None),
        Change::Role {
            application,
            role,
            assigned,
            ..
        } => (
            if *assigned {
                "role_assigned"
            } else {
                "role_removed"
            },
            Some(uuid(application.as_u128())),
            Some(uuid(role.as_u128())),
        ),
    }
}
