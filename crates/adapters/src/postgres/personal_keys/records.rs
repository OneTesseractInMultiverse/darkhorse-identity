use super::*;
pub(super) async fn issue(
    tx: &mut Tx<'_>,
    actor: &Actor,
    request: &Request,
    prepared: authority::Prepared,
    verifier: Verifier,
) -> Result<Record, Error> {
    let expires = prepared.expiry.deadline(request.expiration(), actor.now)?;
    let record = Record {
        id: verifier.id,
        name: request.name().as_str().into(),
        application: request.application(),
        created_ms: actor.now,
        expires_ms: expires,
        active: true,
        grants: prepared.grants,
    };
    sqlx::query("INSERT INTO credentials(id,principal_id,kind) VALUES($1,$2,'personal_key')")
        .bind(uuid(record.id.as_u128()))
        .bind(uuid(actor.principal.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("INSERT INTO personal_keys(credential_id,principal_id,application_id,name,principal_epoch,policy_revision,created_ms,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(uuid(record.id.as_u128())).bind(uuid(actor.principal.as_u128())).bind(uuid(record.application.as_u128())).bind(&record.name).bind(actor.epoch as i64).bind(request.revision() as i64).bind(record.created_ms as i64).bind(record.expires_ms.map(|n|n as i64)).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("INSERT INTO personal_key_verifiers(credential_id,verifier) VALUES($1,$2)")
        .bind(uuid(record.id.as_u128()))
        .bind(verifier.digest.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    for grant in &record.grants {
        sqlx::query("INSERT INTO personal_key_grants(credential_id,application_id,resource_id,capability_ceiling) VALUES($1,$2,$3,$4)")
            .bind(uuid(record.id.as_u128())).bind(uuid(record.application.as_u128())).bind(uuid(grant.resource.as_u128())).bind(projection::encoded(&grant.ceiling)).execute(&mut **tx).await.map_err(storage)?;
    }
    audit(tx, actor, record.id, "issued").await?;
    Ok(record)
}
async fn audit(
    tx: &mut Tx<'_>,
    actor: &Actor,
    key: CredentialId,
    event: &str,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO personal_key_audit(credential_id,principal_id,actor_session_id,event,occurred_ms) VALUES($1,$2,$3,$4,$5)")
        .bind(uuid(key.as_u128())).bind(uuid(actor.principal.as_u128())).bind(uuid(actor.session.as_u128())).bind(event).bind(actor.now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn revoke(tx: &mut Tx<'_>, actor: &Actor, key: CredentialId) -> Result<(), Error> {
    let revoked:bool=sqlx::query_scalar("SELECT c.revoked FROM credentials c JOIN personal_keys k ON k.credential_id=c.id WHERE k.principal_id=$1 AND k.credential_id=$2 FOR UPDATE OF c")
        .bind(uuid(actor.principal.as_u128())).bind(uuid(key.as_u128())).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::NotFound)?;
    if revoked {
        return Ok(());
    }
    sqlx::query("UPDATE credentials SET revoked=true WHERE id=$1")
        .bind(uuid(key.as_u128()))
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    audit(tx, actor, key, "revoked").await
}
pub(super) async fn list(
    tx: &mut Tx<'_>,
    actor: &Actor,
    after: Option<CredentialId>,
) -> Result<Page, Error> {
    let mut rows=sqlx::query("SELECT k.*,c.revoked FROM personal_keys k JOIN credentials c ON c.id=k.credential_id WHERE k.principal_id=$1 AND ($2::uuid IS NULL OR k.credential_id>$2) ORDER BY k.credential_id LIMIT 26")
        .bind(uuid(actor.principal.as_u128())).bind(after.map(|id|uuid(id.as_u128()))).fetch_all(&mut **tx).await.map_err(storage)?;
    let next = continuation(&rows)?;
    rows.truncate(policy::PAGE_SIZE);
    let mut items = Vec::new();
    for row in rows {
        let id = CredentialId::from_u128(identifier(&row, "credential_id")?).map_err(storage)?;
        let grants = grants(tx, id).await?;
        items.push(record(&row, actor, id, grants)?);
    }
    Ok(Page { items, next })
}
fn continuation(rows: &[PgRow]) -> Result<Option<CredentialId>, Error> {
    if rows.len() > policy::PAGE_SIZE {
        Ok(Some(
            CredentialId::from_u128(identifier(&rows[policy::PAGE_SIZE - 1], "credential_id")?)
                .map_err(storage)?,
        ))
    } else {
        Ok(None)
    }
}
fn record(
    row: &PgRow,
    actor: &Actor,
    id: CredentialId,
    grants: Vec<Grant>,
) -> Result<Record, Error> {
    let created_ms = number(row, "created_ms")?;
    let expires_ms = optional_time(row, "expires_ms")?;
    Ok(Record {
        id,
        name: row.try_get("name").map_err(storage)?,
        application: ApplicationId::from_u128(identifier(row, "application_id")?)
            .map_err(storage)?,
        created_ms,
        expires_ms,
        active: !row.try_get::<bool, _>("revoked").map_err(storage)?
            && number(row, "principal_epoch")? == actor.epoch
            && policy::live(created_ms, expires_ms, actor.now),
        grants,
    })
}
async fn grants(tx: &mut Tx<'_>, key: CredentialId) -> Result<Vec<Grant>, Error> {
    let rows=sqlx::query("SELECT resource_id,capability_ceiling FROM personal_key_grants WHERE credential_id=$1 ORDER BY resource_id LIMIT 17")
        .bind(uuid(key.as_u128())).fetch_all(&mut **tx).await.map_err(storage)?;
    if rows.is_empty() || rows.len() > policy::MAX_RESOURCES {
        return Err(Error::Unavailable);
    }
    rows.iter()
        .map(|row| {
            Ok(Grant {
                resource: ResourceId::from_u128(identifier(row, "resource_id")?)
                    .map_err(storage)?,
                ceiling: projection::capabilities(
                    &row.try_get::<Vec<Uuid>, _>("capability_ceiling")
                        .map_err(storage)?,
                )?,
            })
        })
        .collect()
}
pub(super) async fn options(
    tx: &mut Tx<'_>,
    principal: PrincipalId,
    after: Option<ResourceId>,
) -> Result<(Vec<Eligible>, Option<ResourceId>), Error> {
    let mut ids:Vec<Uuid>=sqlx::query_scalar("SELECT r.id FROM protected_resources r JOIN applications a ON a.id=r.application_id WHERE a.active AND ($2::uuid IS NULL OR r.id>$2) AND EXISTS(SELECT 1 FROM principal_roles pr JOIN role_capabilities rc ON rc.role_id=pr.role_id JOIN resource_capabilities exposed ON exposed.capability_id=rc.capability_id AND exposed.resource_id=r.id AND exposed.application_id=r.application_id JOIN capabilities c ON c.id=rc.capability_id AND NOT c.retired WHERE pr.principal_id=$1 AND pr.application_id=r.application_id) ORDER BY r.id LIMIT 26")
        .bind(uuid(principal.as_u128())).bind(after.map(|id|uuid(id.as_u128()))).fetch_all(&mut **tx).await.map_err(storage)?;
    let next = if ids.len() > policy::PAGE_SIZE {
        Some(ResourceId::from_u128(ids[policy::PAGE_SIZE - 1].as_u128()).map_err(storage)?)
    } else {
        None
    };
    ids.truncate(policy::PAGE_SIZE);
    let mut items = Vec::new();
    for id in ids {
        let resource = ResourceId::from_u128(id.as_u128()).map_err(storage)?;
        let policy = projection::load(tx, principal, resource, &BTreeSet::new()).await?;
        let plan = policy.plan(policy.target.application, CapabilitySelection::All)?;
        let caps = descriptions(tx, &plan.ceiling).await?;
        items.push(Eligible {
            application: policy.target.application,
            resource,
            application_name: policy.application_name,
            resource_name: policy.resource_name,
            capabilities: caps,
        });
    }
    Ok((items, next))
}
async fn descriptions(
    tx: &mut Tx<'_>,
    caps: &CapabilitySet,
) -> Result<Vec<darkhorse_application::personal_keys::Capability>, Error> {
    let rows=sqlx::query("SELECT id,permission_key,meaning FROM capabilities WHERE id=ANY($1) ORDER BY permission_key")
        .bind(projection::encoded(caps)).fetch_all(&mut **tx).await.map_err(storage)?;
    rows.iter()
        .map(|r| {
            Ok(darkhorse_application::personal_keys::Capability {
                id: CapabilityId::from_u128(identifier(r, "id")?).map_err(storage)?,
                key: r.try_get("permission_key").map_err(storage)?,
                meaning: r.try_get("meaning").map_err(storage)?,
            })
        })
        .collect()
}
