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
        scopes: row.try_get("scopes").map_err(storage)?,
        resource: row
            .try_get::<Option<Uuid>, _>("resource_id")
            .map_err(storage)?
            .map(|id| ResourceId::from_u128(id.as_u128()).map_err(storage))
            .transpose()?,
        epoch: row
            .try_get::<Option<i64>, _>("principal_epoch")
            .map_err(storage)?
            .map(|n| u64::try_from(n).map_err(storage))
            .transpose()?,
        ceiling: super::super::resource_authority::capabilities(
            &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
                .map_err(storage)?,
        )?,
    })
}
// Identity checks need only the issuing registration and exact callback binding.
// Resource catalogs are loaded separately for resource issuance.
async fn identity_policy(
    tx: &mut Tx<'_>,
    code: &CodeRecord,
) -> Result<darkhorse_domain::oidc::ClientPolicy, Error> {
    let row = sqlx::query("SELECT c.active AND a.active AS active,c.revision,a.revision AS application_revision,ARRAY(SELECT uri FROM client_redirects WHERE client_id=c.id AND uri=$2 LIMIT 1) AS redirects FROM oauth_clients c JOIN applications a ON a.id=c.application_id WHERE c.id=$1")
        .bind(Uuid::from_u128(code.client.as_u128())).bind(&code.redirect)
        .fetch_one(&mut **tx).await.map_err(storage)?;
    Ok(darkhorse_domain::oidc::ClientPolicy {
        active: row.try_get("active").map_err(storage)?,
        revision: number(&row, "revision")?,
        application_revision: number(&row, "application_revision")?,
        redirects: row.try_get("redirects").map_err(storage)?,
        resources: Vec::new(),
    })
}
pub(super) async fn current(tx: &mut Tx<'_>, code: &CodeRecord) -> Result<(), Error> {
    current_scopes(tx, code, &code.scopes).await
}
pub(super) async fn current_scopes(
    tx: &mut Tx<'_>,
    code: &CodeRecord,
    scopes: &[String],
) -> Result<(), Error> {
    tokens::consent(scopes, Some(&code.scopes))?;
    let policy = identity_policy(tx, code).await?;
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
        &policy,
        session,
        &code.redirect,
    )?;
    let approved: Option<Vec<String>> = sqlx::query_scalar("SELECT scopes FROM oauth_consents WHERE principal_id=$1 AND client_id=$2 AND resource=$5 AND client_revision=$3 AND application_revision=$4 FOR SHARE")
        .bind(Uuid::from_u128(code.principal.as_u128())).bind(Uuid::from_u128(code.client.as_u128()))
        .bind(code.client_revision as i64).bind(code.application_revision as i64).bind(code.audience().unwrap_or_default())
        .fetch_optional(&mut **tx).await.map_err(storage)?;
    tokens::consent(scopes, approved.as_deref())
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

pub(super) async fn grant(
    tx: &mut Tx<'_>,
    code: &CodeRecord,
    issuer: &str,
) -> Result<IssuedGrant, Error> {
    match code.audience() {
        None => identity_grant(code, issuer),
        Some(audience) => {
            let plan = super::super::resource_authority::plan(
                tx,
                code.principal,
                code.client,
                &audience,
                &code.scopes,
                Some(&code.ceiling),
            )
            .await?;
            resource_grant(code, &plan, audience)
        }
    }
}
fn identity_grant(code: &CodeRecord, issuer: &str) -> Result<IssuedGrant, Error> {
    Ok(IssuedGrant {
        audience: format!("{issuer}/userinfo"),
        scope: tokens::scope_text(&code.scopes)?,
        claims: tokens::claim_ceiling(&code.scopes)?,
        capabilities: Vec::new(),
        resource: None,
    })
}
fn resource_grant(
    code: &CodeRecord,
    plan: &darkhorse_domain::authorization::IssuancePlan,
    audience: String,
) -> Result<IssuedGrant, Error> {
    if code.epoch != Some(plan.principal_epoch) || code.resource != Some(plan.target.resource) {
        return Err(Error::InvalidGrant);
    }
    Ok(IssuedGrant {
        audience,
        scope: tokens::resource_scope_text(&code.scopes)?,
        claims: Vec::new(),
        capabilities: super::super::resource_authority::encoded(&plan.ceiling),
        resource: Some(Uuid::from_u128(plan.target.resource.as_u128())),
    })
}
