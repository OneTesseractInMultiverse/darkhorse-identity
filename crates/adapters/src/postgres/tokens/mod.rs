use super::{PostgresStore, oidc::authority};
use darkhorse_application::{
    signing::{PublicKey, WrappedKey},
    tokens::*,
};
use darkhorse_domain::{
    identity::{ClientId, PrincipalId, RelyingPartySessionId, ResourceId},
    tokens::{self, CodeFacts, Error, Proof},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod access;
mod management;
mod reads;
mod refresh;
mod relying_party;
type Tx<'a> = Transaction<'a, Postgres>;
pub(super) struct CodeRecord {
    client: ClientId,
    principal: PrincipalId,
    session: [u8; 32],
    client_revision: u64,
    application_revision: u64,
    redirect: String,
    challenge: [u8; 32],
    nonce: Option<String>,
    authenticated: u64,
    created: u64,
    expires: u64,
    consumed: bool,
    scopes: Vec<String>,
    resource: Option<ResourceId>,
    epoch: Option<u64>,
    ceiling: darkhorse_domain::authorization::CapabilitySet,
}
impl CodeRecord {
    fn audience(&self) -> Option<String> {
        self.resource
            .map(|id| format!("urn:darkhorse:resource:{}", Uuid::from_u128(id.as_u128())))
    }
}
impl TokenStore for PostgresStore {
    async fn redeem<S: IdSigner>(
        &self,
        input: Redemption,
        material: Material,
        issuer: &str,
        signer: &S,
    ) -> Result<Tokens, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        reads::client(&mut tx, input.client, input.secret).await?;
        let code = reads::code(&mut tx, input.code).await?;
        reads::client(&mut tx, input.client, input.secret).await?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        tokens::binding(&facts(&code), &proof(&input))?;
        tokens::resource_binding(code.audience().as_deref(), input.resource.as_deref())?;
        if code.consumed {
            revoke(&mut tx, input.code).await?;
            audit(&mut tx, code.principal, code.client, "code_replayed", now).await?;
            tx.commit().await.map_err(storage)?;
            return Err(Error::InvalidGrant);
        }
        reads::current(&mut tx, &code).await?;
        let grant = reads::grant(&mut tx, &code, issuer).await?;
        let key = reads::key(&mut tx, issuer).await?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        tokens::exchange(&facts(&code), &proof(&input), now)?;
        let session = relying_party::bind(&mut tx, &code, issuer, now).await?;
        let id_token = signer
            .sign(key, claims(&code, session, issuer, now))
            .await?;
        reads::client(&mut tx, input.client, input.secret).await?;
        reads::current(&mut tx, &code).await?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        tokens::exchange(&facts(&code), &proof(&input), now)?;
        consume(&mut tx, input.code).await?;
        let refresh = refresh::issue(
            &mut tx,
            input.code,
            &code,
            &grant,
            &material.refresh,
            issuer,
            now,
        )
        .await?;
        let expires = access_expiry(refresh, now)?;
        persist(
            &mut tx,
            input.code,
            material.access.digest,
            now,
            expires,
            refresh.map(|w| w.generation),
            &grant,
        )
        .await?;
        relying_party::audit(&mut tx, &code, session, now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(issued_response(
            material, grant, refresh, id_token, expires, now,
        ))
    }
    async fn userinfo(&self, digest: [u8; 32], issuer: &str) -> Result<UserInfo, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        let access = access::load(&mut tx, digest, issuer, None).await?;
        let profile = access::profile(&mut tx, &access).await?;
        tx.commit().await.map_err(storage)?;
        Ok(profile)
    }
}

fn access_expiry(
    refresh: Option<darkhorse_domain::refresh::Window>,
    now: u64,
) -> Result<u64, Error> {
    match refresh {
        Some(window) => Ok(window.access_expires),
        None => tokens::deadline(now, tokens::ACCESS_MS),
    }
}
fn issued_response(
    material: Material,
    grant: IssuedGrant,
    refresh: Option<darkhorse_domain::refresh::Window>,
    id_token: String,
    expires: u64,
    now: u64,
) -> Tokens {
    Tokens {
        access: material.access.value,
        refresh: refresh.map(|_| material.refresh.value),
        id_token: Some(id_token),
        expires_in: (expires - now) / 1000,
        scope: grant.scope,
    }
}

fn facts(code: &CodeRecord) -> CodeFacts<'_> {
    CodeFacts {
        client: code.client,
        redirect: &code.redirect,
        challenge: code.challenge,
        created_ms: code.created,
        expires_ms: code.expires,
    }
}
fn proof(input: &Redemption) -> Proof<'_> {
    Proof {
        client: input.client,
        redirect: &input.redirect,
        challenge: input.challenge,
    }
}
fn claims(code: &CodeRecord, session: RelyingPartySessionId, issuer: &str, now: u64) -> IdClaims {
    IdClaims {
        issuer: issuer.into(),
        client: code.client,
        subject: code.principal,
        session,
        nonce: code.nonce.clone(),
        authenticated: code.authenticated / 1000,
        issued: now / 1000,
        expires: now / 1000 + tokens::ACCESS_MS / 1000,
    }
}
struct IssuedGrant {
    audience: String,
    scope: String,
    claims: Vec<String>,
    capabilities: Vec<Uuid>,
    resource: Option<Uuid>,
}
async fn consume(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<(), Error> {
    sqlx::query("UPDATE authorization_codes SET consumed=true WHERE digest=$1")
        .bind(digest.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
async fn persist(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    access: [u8; 32],
    now: u64,
    expires: u64,
    generation: Option<u16>,
    grant: &IssuedGrant,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms,resource_id,refresh_generation) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(access.as_slice()).bind(digest.as_slice()).bind(&grant.audience).bind(&grant.scope)
        .bind(&grant.claims).bind(&grant.capabilities).bind(now as i64)
        .bind(expires as i64).bind(grant.resource).bind(generation.map(|n| n as i16))
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn revoke(tx: &mut Tx<'_>, code: [u8; 32]) -> Result<(), Error> {
    refresh::revoke_code(tx, code).await?;
    sqlx::query("UPDATE access_tokens SET revoked=true WHERE code_digest=$1")
        .bind(code.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
pub(in crate::postgres) async fn audit(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    client: ClientId,
    event: &str,
    now: u64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO token_audit(principal_id,client_id,event,occurred_ms) VALUES($1,$2,$3,$4)",
    )
    .bind(Uuid::from_u128(principal.as_u128()))
    .bind(Uuid::from_u128(client.as_u128()))
    .bind(event)
    .bind(now as i64)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}
fn storage<T>(_: T) -> Error {
    Error::Unavailable
}
fn number(row: &PgRow, key: &str) -> Result<u64, Error> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn bytes(row: &PgRow, key: &str) -> Result<[u8; 32], Error> {
    row.try_get::<Vec<u8>, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
pub(in crate::postgres) mod resources;
