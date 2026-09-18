use super::*;
pub(super) struct Stored {
    pub code: CodeRecord,
    pub created: u64,
    pub expires: u64,
    pub scope: String,
    pub ceiling: Vec<String>,
}
pub(super) async fn lookup(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    issuer: &str,
    owner: Option<ClientId>,
) -> Result<[u8; 32], Error> {
    let value:Vec<u8> = sqlx::query_scalar("SELECT t.code_digest FROM access_tokens t JOIN authorization_codes c ON c.digest=t.code_digest WHERE t.digest=$1 AND t.audience=$2 AND ($3::uuid IS NULL OR c.client_id=$3)")
        .bind(digest.as_slice()).bind(format!("{issuer}/userinfo")).bind(owner.map(|id| Uuid::from_u128(id.as_u128())))
        .fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidToken)?;
    value.try_into().map_err(storage)
}
pub(super) async fn load(
    tx: &mut Tx<'_>,
    digest: [u8; 32],
    issuer: &str,
    owner: Option<ClientId>,
) -> Result<Stored, Error> {
    let code_digest = lookup(tx, digest, issuer, owner).await?;
    let code = reads::code_shared(tx, code_digest).await?;
    let row = sqlx::query("SELECT created_ms,expires_ms,scope,claim_ceiling FROM access_tokens WHERE digest=$1 AND NOT revoked FOR SHARE")
        .bind(digest.as_slice()).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::InvalidToken)?;
    reads::current(tx, &code).await.map_err(inactive)?;
    let now = authority::now(tx).await.map_err(storage)?;
    decode(&row, code, now)
}
fn decode(row: &PgRow, code: CodeRecord, now: u64) -> Result<Stored, Error> {
    let created = number(row, "created_ms")?;
    let expires = number(row, "expires_ms")?;
    if !tokens::live(created, expires, now) {
        return Err(Error::InvalidToken);
    }
    Ok(Stored {
        code,
        created,
        expires,
        scope: row.try_get("scope").map_err(storage)?,
        ceiling: row.try_get("claim_ceiling").map_err(storage)?,
    })
}
fn inactive(error: Error) -> Error {
    match error {
        Error::Unavailable => error,
        _ => Error::InvalidToken,
    }
}
pub(super) async fn profile(tx: &mut Tx<'_>, access: &Stored) -> Result<UserInfo, Error> {
    let scopes = access
        .scope
        .split(' ')
        .map(String::from)
        .collect::<Vec<_>>();
    let disclosure = tokens::disclosure(&scopes, &access.ceiling)?;
    let row = sqlx::query("SELECT CASE WHEN $2 THEN first_name END AS given, CASE WHEN $2 THEN last_name END AS family, CASE WHEN $3 THEN email END AS email FROM principals WHERE id=$1")
        .bind(Uuid::from_u128(access.code.principal.as_u128())).bind(disclosure.profile).bind(disclosure.email)
        .fetch_one(&mut **tx).await.map_err(storage)?;
    project(&row, access.code.principal)
}
fn project(row: &PgRow, subject: PrincipalId) -> Result<UserInfo, Error> {
    let given: Option<String> = row.try_get("given").map_err(storage)?;
    let family: Option<String> = row.try_get("family").map_err(storage)?;
    Ok(UserInfo {
        subject,
        profile: given
            .zip(family)
            .map(|(given, family)| Names { given, family }),
        email: row.try_get("email").map_err(storage)?,
    })
}
pub(super) fn metadata(stored: Stored) -> ActiveToken {
    ActiveToken {
        subject: stored.code.principal,
        client: stored.code.client,
        scope: stored.scope,
        issued: stored.created / 1000,
        expires: stored.expires / 1000,
    }
}
