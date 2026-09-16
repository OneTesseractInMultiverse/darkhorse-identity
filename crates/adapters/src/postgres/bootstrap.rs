use darkhorse_application::bootstrap::{BootstrapError, NewAdministrator};
use darkhorse_domain::identity::PrincipalId;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub(super) async fn bootstrap(
    pool: &PgPool,
    administrator: NewAdministrator,
) -> Result<PrincipalId, BootstrapError> {
    let mut tx = pool.begin().await.map_err(storage)?;
    let consumed: bool =
        sqlx::query_scalar("SELECT bootstrapped FROM security_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
    if consumed {
        return Err(BootstrapError::AlreadyInitialized);
    }
    insert_administrator(&mut tx, &administrator)
        .await
        .map_err(storage)?;
    sqlx::query("UPDATE security_state SET bootstrapped = true WHERE singleton")
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    tx.commit().await.map_err(storage)?;
    Ok(administrator.credential.principal_id)
}

async fn insert_administrator(
    tx: &mut Transaction<'_, Postgres>,
    administrator: &NewAdministrator,
) -> Result<(), sqlx::Error> {
    let principal = Uuid::from_u128(administrator.credential.principal_id.as_u128());
    let credential = Uuid::from_u128(administrator.credential.credential_id.as_u128());
    sqlx::query(
        "INSERT INTO principals (id, email, first_name, last_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(principal)
    .bind(administrator.profile.email())
    .bind(administrator.profile.first_name())
    .bind(administrator.profile.last_name())
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO credentials (id, principal_id, kind) VALUES ($1, $2, 'password')")
        .bind(credential)
        .bind(principal)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO password_credentials (credential_id, verifier) VALUES ($1, $2)")
        .bind(credential)
        .bind(&administrator.credential.verifier)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO platform_administrators (principal_id) VALUES ($1)")
        .bind(principal)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO security_audit (event, principal_id, principal_revision, credential_epoch) VALUES ('bootstrap', $1, 0, 0)").bind(principal).execute(&mut **tx).await?;
    Ok(())
}

fn storage(_: sqlx::Error) -> BootstrapError {
    BootstrapError::Storage
}
