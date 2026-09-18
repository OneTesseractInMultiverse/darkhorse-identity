use super::super::resource_authority::{self, StoredGrant};
use super::*;
use darkhorse_application::resource_servers::{ActiveResourceToken, Probe, ResourceTokenStore};
use darkhorse_domain::registration::secret_live;
struct Authentication {
    created: u64,
    expires: Option<u64>,
}
struct Access {
    grant: StoredGrant,
    scope: String,
    scopes: Vec<String>,
}
impl ResourceTokenStore for PostgresStore {
    async fn introspect_resource(
        &self,
        input: Probe,
        issuer: &str,
    ) -> Result<Option<ActiveResourceToken>, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        let authentication = authenticate(&mut tx, &input).await?;
        let result = inspect(&mut tx, &input, issuer).await;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        valid_authentication(&authentication, now)?;
        let active = inactive(result)?;
        tx.commit().await.map_err(storage)?;
        Ok(active)
    }
}
fn valid_authentication(authentication: &Authentication, now: u64) -> Result<(), Error> {
    if !secret_live(authentication.created, authentication.expires, false, now) {
        return Err(Error::InvalidClient);
    }
    Ok(())
}
async fn authenticate(tx: &mut Tx<'_>, input: &Probe) -> Result<Authentication, Error> {
    let row=sqlx::query("SELECT s.created_ms,s.expires_ms FROM resource_introspection r JOIN applications a ON a.id=r.application_id JOIN resource_introspection_secrets s ON s.resource_id=r.resource_id WHERE r.resource_id=$1 AND s.verifier=$2 AND r.active AND a.active AND NOT s.retired")
        .bind(Uuid::from_u128(input.resource.as_u128())).bind(input.secret.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidClient)?;
    let auth = Authentication {
        created: number(&row, "created_ms")?,
        expires: row
            .try_get::<Option<i64>, _>("expires_ms")
            .map_err(storage)?
            .map(|n| u64::try_from(n).map_err(storage))
            .transpose()?,
    };
    let now = authority::now(tx).await.map_err(storage)?;
    valid_authentication(&auth, now)?;
    Ok(auth)
}
async fn inspect(
    tx: &mut Tx<'_>,
    input: &Probe,
    issuer: &str,
) -> Result<ActiveResourceToken, Error> {
    let token = input.token.ok_or(Error::InvalidToken)?;
    let digest:Vec<u8>=sqlx::query_scalar("SELECT t.code_digest FROM access_tokens t WHERE t.digest=$1 AND t.resource_id=$2 AND t.audience='urn:darkhorse:resource:'||$2::uuid::text AND EXISTS(SELECT 1 FROM provider_state WHERE issuer=$3)")
        .bind(token.as_slice()).bind(Uuid::from_u128(input.resource.as_u128())).bind(issuer).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidToken)?;
    let code = reads::code_shared(tx, digest.try_into().map_err(storage)?).await?;
    let row=sqlx::query("SELECT credential_id,resource_id,capability_ceiling,scope,created_ms,expires_ms FROM access_tokens WHERE digest=$1 AND NOT revoked FOR SHARE")
        .bind(token.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidToken)?;
    reads::current(tx, &code).await?;
    let stored = decode(&row, &code)?;
    let policy = resource_authority::load(
        tx,
        code.principal,
        code.client,
        &code.audience().ok_or(Error::InvalidToken)?,
        &stored.scopes,
        Some(&stored.grant.ceiling),
    )
    .await?;
    let now = authority::now(tx).await.map_err(storage)?;
    active(&code, stored, &policy, now)
}
fn decode(row: &PgRow, code: &CodeRecord) -> Result<Access, Error> {
    let scope: String = row.try_get("scope").map_err(storage)?;
    Ok(Access {
        scopes: scope.split(' ').map(String::from).collect(),
        scope,
        grant: StoredGrant {
            credential: darkhorse_domain::identity::CredentialId::from_u128(
                row.try_get::<Uuid, _>("credential_id")
                    .map_err(storage)?
                    .as_u128(),
            )
            .map_err(storage)?,
            resource: ResourceId::from_u128(
                row.try_get::<Uuid, _>("resource_id")
                    .map_err(storage)?
                    .as_u128(),
            )
            .map_err(storage)?,
            epoch: code.epoch.ok_or(Error::InvalidToken)?,
            ceiling: resource_authority::capabilities(
                &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
                    .map_err(storage)?,
            )?,
            created: number(row, "created_ms")?,
            expires: number(row, "expires_ms")?,
        },
    })
}
fn active(
    code: &CodeRecord,
    stored: Access,
    policy: &resource_authority::Policy,
    now: u64,
) -> Result<ActiveResourceToken, Error> {
    let capabilities = policy.evaluate(&stored.grant, now)?;
    if capabilities.is_empty() {
        return Err(Error::InvalidToken);
    }
    Ok(ActiveResourceToken {
        resource: stored.grant.resource,
        capabilities,
        token: ActiveToken {
            subject: code.principal,
            client: code.client,
            scope: stored.scope,
            issued: stored.grant.created / 1000,
            expires: stored.grant.expires / 1000,
        },
    })
}
fn inactive(
    result: Result<ActiveResourceToken, Error>,
) -> Result<Option<ActiveResourceToken>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::Unavailable) => Err(Error::Unavailable),
        Err(_) => Ok(None),
    }
}
