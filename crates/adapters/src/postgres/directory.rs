use darkhorse_application::directory::{AccountRecord, DirectoryFailure};
use darkhorse_domain::{
    AccountStatus,
    directory::{AccountAction, AccountChange, AccountSnapshot, Profile, plan_change},
    identity::PrincipalId,
};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

const ACCOUNT: &str = "SELECT p.*, EXISTS (SELECT 1 FROM platform_administrators a WHERE a.principal_id = p.id) AS administrator, EXISTS (SELECT 1 FROM eligible_administrators e WHERE e.principal_id = p.id) AS eligible_administrator FROM principals p WHERE p.id = $1";

pub(super) async fn account(
    pool: &PgPool,
    id: PrincipalId,
) -> Result<AccountRecord, DirectoryFailure> {
    let row = sqlx::query(ACCOUNT)
        .bind(Uuid::from_u128(id.as_u128()))
        .fetch_optional(pool)
        .await
        .map_err(unavailable)?
        .ok_or(DirectoryFailure::NotFound)?;
    map_account(id, &row)
}

pub(super) async fn change(
    pool: &PgPool,
    id: PrincipalId,
    expected_revision: u64,
    action: AccountAction,
) -> Result<Option<AccountChange>, DirectoryFailure> {
    let mut tx = pool.begin().await.map_err(unavailable)?;
    sqlx::query("SELECT singleton FROM security_state WHERE singleton FOR UPDATE")
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    let result = change_locked(&mut tx, id, expected_revision, action).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(result)
}

/// The caller owns the transaction and holds the exclusive security fence.
pub(super) async fn change_locked(
    tx: &mut Transaction<'_, Postgres>,
    id: PrincipalId,
    expected_revision: u64,
    action: AccountAction,
) -> Result<Option<AccountChange>, DirectoryFailure> {
    let current = locked_account(tx, id).await?;
    if current.revision != expected_revision {
        return Err(DirectoryFailure::Conflict);
    }
    let administrators: i64 = sqlx::query_scalar("SELECT count(*) FROM eligible_administrators")
        .fetch_one(&mut **tx)
        .await
        .map_err(unavailable)?;
    let snapshot = snapshot(&current, administrators);
    let change = plan_change(snapshot, action).map_err(DirectoryFailure::Policy)?;
    if let Some(change) = change {
        persist(tx, id, change, action).await.map_err(unavailable)?;
    }
    Ok(change)
}

pub(super) async fn locked_account(
    tx: &mut Transaction<'_, Postgres>,
    id: PrincipalId,
) -> Result<AccountRecord, DirectoryFailure> {
    let row = sqlx::query("SELECT p.*, EXISTS (SELECT 1 FROM platform_administrators a WHERE a.principal_id = p.id) AS administrator, EXISTS (SELECT 1 FROM eligible_administrators e WHERE e.principal_id = p.id) AS eligible_administrator FROM principals p WHERE p.id = $1 FOR UPDATE OF p")
        .bind(Uuid::from_u128(id.as_u128())).fetch_optional(&mut **tx).await.map_err(unavailable)?.ok_or(DirectoryFailure::NotFound)?;
    map_account(id, &row)
}

fn snapshot(record: &AccountRecord, administrators: i64) -> AccountSnapshot {
    AccountSnapshot {
        status: record.status,
        credential_epoch: record.credential_epoch,
        revision: record.revision,
        eligible_administrator: record.eligible_administrator,
        eligible_administrators: administrators as u64,
    }
}

async fn persist(
    tx: &mut Transaction<'_, Postgres>,
    id: PrincipalId,
    change: AccountChange,
    action: AccountAction,
) -> Result<(), sqlx::Error> {
    let principal = Uuid::from_u128(id.as_u128());
    sqlx::query("UPDATE principals SET active=$2, credential_epoch=$3, revision=$4 WHERE id=$1")
        .bind(principal)
        .bind(change.status == AccountStatus::Active)
        .bind(change.credential_epoch as i64)
        .bind(change.revision as i64)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO security_audit (event, principal_id, principal_revision, credential_epoch) VALUES ($1, $2, $3, $4)")
        .bind(event(action)).bind(principal).bind(change.revision as i64).bind(change.credential_epoch as i64).execute(&mut **tx).await?;
    Ok(())
}

fn event(action: AccountAction) -> &'static str {
    match action {
        AccountAction::RevokeAll => "account.revoked",
        AccountAction::SetStatus(AccountStatus::Inactive) => "account.deactivated",
        AccountAction::SetStatus(AccountStatus::Active) => "account.reactivated",
    }
}

fn map_account(id: PrincipalId, row: &PgRow) -> Result<AccountRecord, DirectoryFailure> {
    let profile = Profile::new(
        row.try_get("email").map_err(unavailable)?,
        row.try_get("first_name").map_err(unavailable)?,
        row.try_get("last_name").map_err(unavailable)?,
    )
    .map_err(|_| DirectoryFailure::Unavailable)?;
    Ok(AccountRecord {
        id,
        profile,
        status: if row.try_get("active").map_err(unavailable)? {
            AccountStatus::Active
        } else {
            AccountStatus::Inactive
        },
        credential_epoch: row
            .try_get::<i64, _>("credential_epoch")
            .map_err(unavailable)? as u64,
        revision: row.try_get::<i64, _>("revision").map_err(unavailable)? as u64,
        administrator: row.try_get("administrator").map_err(unavailable)?,
        eligible_administrator: row.try_get("eligible_administrator").map_err(unavailable)?,
    })
}

fn unavailable(_: sqlx::Error) -> DirectoryFailure {
    DirectoryFailure::Unavailable
}
