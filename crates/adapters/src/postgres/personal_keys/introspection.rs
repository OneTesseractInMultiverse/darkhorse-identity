use super::super::tokens::resources;
use super::*;
use darkhorse_domain::tokens::Error as TokenError;
impl Introspection for PostgresStore {
    async fn introspect_key(
        &self,
        probe: Probe,
        issuer: &str,
    ) -> Result<Option<Active>, TokenError> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        lock(&mut tx, false).await.map_err(unavailable)?;
        let auth = resources::authenticate(
            &mut tx,
            &darkhorse_application::resource_servers::Probe {
                resource: probe.resource,
                secret: probe.secret,
                token: None,
            },
        )
        .await?;
        let result = inspect(&mut tx, &probe, issuer).await;
        let now = sessions::now(&mut tx).await.map_err(unavailable)?;
        resources::valid_authentication(&auth, now)?;
        let active = inactive(result)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(active)
    }
}
async fn inspect(tx: &mut Tx<'_>, probe: &Probe, issuer: &str) -> Result<Active, TokenError> {
    let digest = probe.key.ok_or(TokenError::InvalidToken)?;
    let row=sqlx::query("SELECT k.*,c.revoked,g.resource_id,g.capability_ceiling FROM personal_key_verifiers v JOIN personal_keys k ON k.credential_id=v.credential_id JOIN credentials c ON c.id=k.credential_id JOIN personal_key_grants g ON g.credential_id=k.credential_id WHERE v.verifier=$1 AND g.resource_id=$2 AND EXISTS(SELECT 1 FROM provider_state WHERE issuer=$3) FOR SHARE OF c")
        .bind(digest.as_slice()).bind(uuid(probe.resource.as_u128())).bind(issuer).fetch_optional(&mut **tx).await.map_err(unavailable)?.ok_or(TokenError::InvalidToken)?;
    let grant = decode(&row).map_err(unavailable)?;
    let policy = projection::load(tx, grant.subject, probe.resource, &grant.ceiling)
        .await
        .map_err(policy_error)?;
    let now = sessions::now(tx).await.map_err(unavailable)?;
    evaluate(
        &policy,
        grant,
        number(&row, "created_ms").map_err(unavailable)?,
        optional_time(&row, "expires_ms").map_err(unavailable)?,
        now,
    )
}
fn decode(row: &PgRow) -> Result<CredentialGrant, Error> {
    Ok(CredentialGrant {
        credential: CredentialId::from_u128(identifier(row, "credential_id")?).map_err(storage)?,
        subject: PrincipalId::from_u128(identifier(row, "principal_id")?).map_err(storage)?,
        target: Target {
            application: ApplicationId::from_u128(identifier(row, "application_id")?)
                .map_err(storage)?,
            resource: ResourceId::from_u128(identifier(row, "resource_id")?).map_err(storage)?,
        },
        revoked: row.try_get("revoked").map_err(storage)?,
        principal_epoch: number(row, "principal_epoch")?,
        valid_from: number(row, "created_ms")? / 1000,
        expires_at: optional_time(row, "expires_ms")?.map(|n| n / 1000),
        ceiling: projection::capabilities(
            &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
                .map_err(storage)?,
        )?,
        delegation: Delegation::PersonalKey,
    })
}
fn evaluate(
    policy: &projection::Policy,
    grant: CredentialGrant,
    created: u64,
    expires: Option<u64>,
    now: u64,
) -> Result<Active, TokenError> {
    if !policy::live(created, expires, now) {
        return Err(TokenError::InvalidToken);
    }
    let capabilities = effective_capabilities(
        &policy.catalog,
        &Evaluation {
            principal: &policy.principal,
            credential: &grant,
            target: policy.target,
            now: now / 1000,
        },
    )
    .map_err(|_| TokenError::InvalidToken)?;
    if capabilities.is_empty() {
        return Err(TokenError::InvalidToken);
    }
    Ok(Active {
        credential: grant.credential,
        subject: grant.subject,
        resource: grant.target.resource,
        capabilities,
        issued: grant.valid_from,
        expires: grant.expires_at,
    })
}
fn inactive(result: Result<Active, TokenError>) -> Result<Option<Active>, TokenError> {
    match result {
        Ok(active) => Ok(Some(active)),
        Err(TokenError::Unavailable) => Err(TokenError::Unavailable),
        Err(_) => Ok(None),
    }
}
fn policy_error(error: Error) -> TokenError {
    match error {
        Error::Unavailable => TokenError::Unavailable,
        _ => TokenError::InvalidToken,
    }
}
fn unavailable<T>(_: T) -> TokenError {
    TokenError::Unavailable
}
