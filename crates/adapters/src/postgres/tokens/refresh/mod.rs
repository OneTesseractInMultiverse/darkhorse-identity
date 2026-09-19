use super::*;
use darkhorse_application::refresh::{RefreshStore, Request};
use darkhorse_domain::refresh::{self as policy, Family, Member, Step, Window};
mod maintenance;
mod records;

struct Stored {
    family: Family,
    member: Member,
    scopes: Vec<String>,
    ceiling: darkhorse_domain::authorization::CapabilitySet,
}
pub(super) async fn issue(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    code: &CodeRecord,
    grant: &IssuedGrant,
    refresh: &Opaque,
    issuer: &str,
    now: u64,
) -> Result<Option<Window>, Error> {
    let enabled: bool = sqlx::query_scalar("SELECT refresh_tokens FROM oauth_clients WHERE id=$1")
        .bind(Uuid::from_u128(code.client.as_u128()))
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
    let Some((family, window)) = initial(code, enabled, now)? else {
        return Ok(None);
    };
    records::family(tx, digest, issuer, &family).await?;
    records::insert(tx, digest, refresh.digest, grant, now, window).await?;
    records::audit(tx, digest, 0, "refresh_issued", now).await?;
    Ok(Some(window))
}
fn initial(code: &CodeRecord, enabled: bool, now: u64) -> Result<Option<(Family, Window)>, Error> {
    if !enabled {
        return Ok(None);
    }
    let family = policy::initial(code.authenticated, now)?;
    Ok(Some((family, policy::window(&family, 0, now)?)))
}
impl RefreshStore for PostgresStore {
    async fn refresh(
        &self,
        input: Request,
        material: Material,
        issuer: &str,
    ) -> Result<Tokens, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        reads::client(&mut tx, input.client, input.secret).await?;
        let digest = records::owned(&mut tx, input.digest, input.client, issuer).await?;
        let code = reads::code(&mut tx, digest).await?;
        let stored = records::load(&mut tx, input.digest).await?;
        reads::client(&mut tx, input.client, input.secret).await?;
        tokens::resource_binding(code.audience().as_deref(), input.resource.as_deref())?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        if policy::rotate(&stored.family, &stored.member, now)? == Step::Replay {
            records::revoke(
                &mut tx,
                digest,
                stored.member.generation,
                "refresh_replayed",
                now,
            )
            .await?;
            tx.commit().await.map_err(storage)?;
            return Err(Error::InvalidGrant);
        }
        let result = rotate_live(&mut tx, digest, code, &stored, &input, material, issuer).await?;
        tx.commit().await.map_err(storage)?;
        Ok(result)
    }
}
async fn rotate_live(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    code: CodeRecord,
    stored: &Stored,
    input: &Request,
    material: Material,
    issuer: &str,
) -> Result<Tokens, Error> {
    let code = narrowed(code, stored, input.scopes.as_deref())?;
    reads::current(tx, &code).await?;
    let grant = reads::grant(tx, &code, issuer).await?;
    // Recheck time-sensitive facts after policy reads and any lock waits.
    reads::client(tx, input.client, input.secret).await?;
    reads::current(tx, &code).await?;
    let now = authority::now(tx).await.map_err(storage)?;
    let window = next(stored, now)?;
    records::consume(tx, input.digest).await?;
    records::insert(tx, digest, material.refresh.digest, &grant, now, window).await?;
    persist(
        tx,
        digest,
        material.access.digest,
        now,
        window.access_expires,
        Some(window.generation),
        &grant,
    )
    .await?;
    records::audit(tx, digest, window.generation, "refresh_rotated", now).await?;
    Ok(response(material, grant, window, now))
}
fn narrowed(
    mut code: CodeRecord,
    stored: &Stored,
    requested: Option<&[String]>,
) -> Result<CodeRecord, Error> {
    code.scopes = policy::scopes(&stored.scopes, requested, code.audience().as_deref())?;
    code.ceiling = stored.ceiling.clone();
    Ok(code)
}
fn next(stored: &Stored, now: u64) -> Result<Window, Error> {
    match policy::rotate(&stored.family, &stored.member, now)? {
        Step::Next(window) => Ok(window),
        Step::Replay => Err(Error::InvalidGrant),
    }
}
fn response(material: Material, grant: IssuedGrant, window: Window, now: u64) -> Tokens {
    Tokens {
        access: material.access.value,
        refresh: Some(material.refresh.value),
        id_token: None,
        expires_in: (window.access_expires - now) / 1000,
        scope: grant.scope,
    }
}
pub(super) async fn revoke_code(tx: &mut Tx<'_>, code: [u8; 32]) -> Result<(), Error> {
    let now = authority::now(tx).await.map_err(storage)?;
    records::revoke(tx, code, 0, "refresh_revoked", now).await
}
pub(super) async fn revoke_token(
    tx: &mut Tx<'_>,
    token: [u8; 32],
    client: ClientId,
    secret: [u8; 32],
    issuer: &str,
) -> Result<(), Error> {
    let digest = match records::owned(tx, token, client, issuer).await {
        Ok(digest) => digest,
        Err(Error::InvalidGrant) => return Ok(()),
        Err(error) => return Err(error),
    };
    reads::code(tx, digest).await?;
    let stored = records::load(tx, token).await?;
    reads::client(tx, client, secret).await?;
    let now = authority::now(tx).await.map_err(storage)?;
    records::revoke(tx, digest, stored.member.generation, "refresh_revoked", now).await
}
