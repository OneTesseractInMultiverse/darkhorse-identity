use super::*;
pub(super) async fn client(tx: &mut Tx<'_>, id: ClientId, secret: [u8; 32]) -> Result<(), Error> {
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_client_secrets s JOIN oauth_clients c ON c.id=s.client_id JOIN applications a ON a.id=c.application_id WHERE c.id=$1 AND s.verifier=$2 AND c.active AND a.active AND NOT s.retired AND s.created_ms<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND (s.expires_ms IS NULL OR s.expires_ms>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))")
  .bind(Uuid::from_u128(id.as_u128())).bind(secret.as_slice()).fetch_one(&mut **tx).await.map_err(storage)?;
    if !valid {
        return Err(Error::InvalidClient);
    }
    Ok(())
}
pub(super) async fn code(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<CodeRecord, Error> {
    let row = sqlx::query("SELECT * FROM authorization_codes WHERE digest=$1 FOR UPDATE")
        .bind(digest.as_slice())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(Error::InvalidGrant)?;
    decode(&row)
}
pub(super) async fn code_shared(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<CodeRecord, Error> {
    let row = sqlx::query("SELECT * FROM authorization_codes WHERE digest=$1 FOR SHARE")
        .bind(digest.as_slice())
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
    decode(&row)
}
fn decode(row: &PgRow) -> Result<CodeRecord, Error> {
    Ok(CodeRecord {
        client: ClientId::from_u128(
            row.try_get::<Uuid, _>("client_id")
                .map_err(storage)?
                .as_u128(),
        )
        .map_err(storage)?,
        principal: PrincipalId::from_u128(
            row.try_get::<Uuid, _>("principal_id")
                .map_err(storage)?
                .as_u128(),
        )
        .map_err(storage)?,
        session: bytes(row, "session_digest")?,
        client_revision: number(row, "client_revision")?,
        application_revision: number(row, "application_revision")?,
        redirect: row.try_get("redirect_uri").map_err(storage)?,
        challenge: bytes(row, "challenge")?,
        nonce: row.try_get("nonce").map_err(storage)?,
        authenticated: number(row, "authenticated_ms")?,
        created: number(row, "created_ms")?,
        expires: number(row, "expires_ms")?,
        consumed: row.try_get("consumed").map_err(storage)?,
    })
}
pub(super) async fn current(tx: &mut Tx<'_>, code: &CodeRecord) -> Result<(), Error> {
    let catalog = records::catalog(tx, code.client).await.map_err(convert)?;
    let session = authority::session(tx, Some(code.session))
        .await
        .map_err(convert)?;
    tokens::current(
        &tokens::Grant {
            client_revision: code.client_revision,
            application_revision: code.application_revision,
            principal: code.principal,
            authenticated_ms: code.authenticated,
        },
        &catalog.policy,
        session,
        &code.redirect,
    )
}

pub(super) async fn key(tx: &mut Tx<'_>, issuer: &str) -> Result<WrappedKey, Error> {
    let last: i64 = sqlx::query_scalar(
        "SELECT last_ms FROM provider_state WHERE issuer=$1 AND NOT pg_is_in_recovery() FOR SHARE",
    )
    .bind(issuer)
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?
    .ok_or(Error::Unavailable)?;
    let now = authority::now(tx).await.map_err(storage)?;
    if now < last as u64 {
        return Err(Error::Unavailable);
    }
    let row=sqlx::query("SELECT kid,n,e,nonce,ciphertext FROM signing_keys WHERE phase='active' AND activated_ms<=$1").bind(now as i64).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::Unavailable)?;
    Ok(WrappedKey {
        public: PublicKey {
            kid: row.try_get("kid").map_err(storage)?,
            n: row.try_get("n").map_err(storage)?,
            e: row.try_get("e").map_err(storage)?,
        },
        nonce: row
            .try_get::<Vec<u8>, _>("nonce")
            .map_err(storage)?
            .try_into()
            .map_err(storage)?,
        ciphertext: row.try_get("ciphertext").map_err(storage)?,
    })
}
fn convert(error: darkhorse_domain::oidc::Error) -> Error {
    match error {
        darkhorse_domain::oidc::Error::Unavailable => Error::Unavailable,
        _ => Error::InvalidGrant,
    }
}
