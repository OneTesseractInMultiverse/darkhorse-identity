use super::{
    PostgresStore,
    oidc::{authority, records},
};
use darkhorse_application::{
    signing::{PublicKey, WrappedKey},
    tokens::*,
};
use darkhorse_domain::{
    identity::{ClientId, PrincipalId},
    tokens::{self, CodeFacts, Error, Proof},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;
mod reads;
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
}
impl TokenStore for PostgresStore {
    async fn redeem<S: IdSigner>(
        &self,
        input: Redemption,
        access: Opaque,
        issuer: &str,
        signer: &S,
    ) -> Result<Tokens, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        reads::client(&mut tx, input.client, input.secret).await?;
        let code = reads::code(&mut tx, input.code).await?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        tokens::binding(&facts(&code), &proof(&input))?;
        if code.consumed {
            revoke(&mut tx, input.code).await?;
            audit(&mut tx, code.principal, code.client, "code_replayed", now).await?;
            tx.commit().await.map_err(storage)?;
            return Err(Error::InvalidGrant);
        }
        reads::current(&mut tx, &code).await?;
        let key = reads::key(&mut tx, issuer).await?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        tokens::exchange(&facts(&code), &proof(&input), now)?;
        let id_token = signer.sign(key, claims(&code, issuer, now)).await?;
        persist(&mut tx, input.code, access.digest, issuer, now).await?;
        audit(&mut tx, code.principal, code.client, "code_redeemed", now).await?;
        tx.commit().await.map_err(storage)?;
        Ok(Tokens {
            access: access.value,
            id_token,
            expires_in: tokens::ACCESS_MS / 1000,
        })
    }
    async fn userinfo(&self, digest: [u8; 32], issuer: &str) -> Result<PrincipalId, Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        let code_digest: Vec<u8> = sqlx::query_scalar(
            "SELECT code_digest FROM access_tokens WHERE digest=$1 AND audience=$2",
        )
        .bind(digest.as_slice())
        .bind(format!("{issuer}/userinfo"))
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .ok_or(Error::InvalidToken)?;
        let code = reads::code_shared(&mut tx, code_digest.try_into().map_err(storage)?).await?;
        let row=sqlx::query("SELECT created_ms,expires_ms FROM access_tokens WHERE digest=$1 AND NOT revoked FOR SHARE")
   .bind(digest.as_slice()).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or(Error::InvalidToken)?;
        let now = authority::now(&mut tx).await.map_err(storage)?;
        if !tokens::live(
            number(&row, "created_ms")?,
            number(&row, "expires_ms")?,
            now,
        ) {
            return Err(Error::InvalidToken);
        }
        reads::current(&mut tx, &code).await.map_err(|e| {
            if e == Error::Unavailable {
                e
            } else {
                Error::InvalidToken
            }
        })?;
        tx.commit().await.map_err(storage)?;
        Ok(code.principal)
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
fn claims(code: &CodeRecord, issuer: &str, now: u64) -> IdClaims {
    IdClaims {
        issuer: issuer.into(),
        client: code.client,
        subject: code.principal,
        nonce: code.nonce.clone(),
        authenticated: code.authenticated / 1000,
        issued: now / 1000,
        expires: now / 1000 + tokens::ACCESS_MS / 1000,
    }
}
async fn persist(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    access: [u8; 32],
    issuer: &str,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("UPDATE authorization_codes SET consumed=true WHERE digest=$1")
        .bind(digest.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("INSERT INTO access_tokens(digest,code_digest,audience,scope,claim_ceiling,capability_ceiling,created_ms,expires_ms) VALUES($1,$2,$3,'openid',ARRAY['sub'],ARRAY[]::uuid[],$4,$5)")
  .bind(access.as_slice()).bind(digest.as_slice()).bind(format!("{issuer}/userinfo")).bind(now as i64).bind(tokens::deadline(now,tokens::ACCESS_MS)? as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn revoke(tx: &mut Tx<'_>, code: [u8; 32]) -> Result<(), Error> {
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
