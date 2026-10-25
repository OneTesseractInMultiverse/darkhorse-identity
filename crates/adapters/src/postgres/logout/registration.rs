use super::*;
use crate::postgres::registration::authority;
use darkhorse_application::logout::registration::*;
use darkhorse_domain::registration::{RegistrationError as Error, next_revision};

impl Registration for PostgresStore {
    async fn logout_configuration(&self, actor: [u8; 32], target: Target) -> Result<Record, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        crate::postgres::sessions::lock(&mut tx, false)
            .await
            .map_err(storage)?;
        authority::actor(&mut tx, actor, false).await?;
        exists(&mut tx, target).await?;
        let configuration = current(&mut tx, target).await?;
        let rows = sqlx::query("SELECT id,state,attempts,created_ms FROM logout_deliveries WHERE client_id=$1 ORDER BY created_ms DESC,id DESC LIMIT 20")
            .bind(Uuid::from_u128(target.client.as_u128())).fetch_all(&mut *tx).await.map_err(storage)?;
        let deliveries = rows.iter().map(status).collect::<Result<Vec<_>, _>>()?;
        authority::actor(&mut tx, actor, false).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Record {
            configuration,
            deliveries,
        })
    }
    async fn configure_logout(
        &self,
        actor: [u8; 32],
        target: Target,
        configuration: Configuration,
    ) -> Result<Configuration, Error> {
        validate(&configuration)?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await?;
        let (principal, now) = authority::actor(&mut tx, actor, true).await?;
        exists(&mut tx, target).await?;
        let current = current(&mut tx, target).await?;
        let revision = next_revision(current.revision, configuration.revision)?;
        let client = Uuid::from_u128(target.client.as_u128());
        let changed =
            sqlx::query("INSERT INTO logout_destinations(client_id,revision,uri) VALUES($1,$2,$3)")
                .bind(client)
                .bind(revision as i64)
                .bind(&configuration.uri)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        one(changed.rows_affected()).map_err(storage)?;
        audit(
            &mut tx,
            client,
            None,
            revision as i64,
            "configured",
            now as i64,
            Some(Uuid::from_u128(principal.as_u128())),
        )
        .await
        .map_err(storage)?;
        authority::actor(&mut tx, actor, true).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Configuration {
            revision,
            uri: configuration.uri,
        })
    }
}
fn validate(configuration: &Configuration) -> Result<(), Error> {
    if let Some(uri) = &configuration.uri {
        crate::logout::destination(uri)?;
    }
    Ok(())
}
async fn exists(tx: &mut Tx<'_>, target: Target) -> Result<(), Error> {
    sqlx::query("SELECT id FROM oauth_clients WHERE id=$1 AND application_id=$2")
        .bind(Uuid::from_u128(target.client.as_u128()))
        .bind(Uuid::from_u128(target.application.as_u128()))
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    Ok(())
}
async fn current(tx: &mut Tx<'_>, target: Target) -> Result<Configuration, Error> {
    let row = sqlx::query("SELECT revision,uri FROM logout_destinations WHERE client_id=$1 ORDER BY revision DESC LIMIT 1")
        .bind(Uuid::from_u128(target.client.as_u128())).fetch_optional(&mut **tx).await.map_err(storage)?;
    row.map(decode).transpose().map(|value| {
        value.unwrap_or(Configuration {
            revision: 0,
            uri: None,
        })
    })
}
fn decode(row: PgRow) -> Result<Configuration, Error> {
    Ok(Configuration {
        revision: row
            .try_get::<i64, _>("revision")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        uri: row.try_get("uri").map_err(storage)?,
    })
}
fn status(row: &PgRow) -> Result<DeliveryStatus, Error> {
    Ok(DeliveryStatus {
        id: darkhorse_domain::identity::LogoutDeliveryId::from_u128(
            row.try_get::<Uuid, _>("id").map_err(storage)?.as_u128(),
        )
        .map_err(storage)?,
        state: row.try_get("state").map_err(storage)?,
        attempts: row
            .try_get::<i16, _>("attempts")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        created_ms: row
            .try_get::<i64, _>("created_ms")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
    })
}
fn storage(_: impl std::fmt::Debug) -> Error {
    Error::Unavailable
}
