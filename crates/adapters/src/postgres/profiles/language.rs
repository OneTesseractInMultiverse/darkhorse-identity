use super::*;
use darkhorse_domain::localization::Locale;
pub(super) async fn update(
    store: &PostgresStore,
    digest: [u8; 32],
    expected: u64,
    locale: Option<Locale>,
) -> Result<Profile, Error> {
    let mut tx = store.pool.begin().await.map_err(storage)?;
    lock(&mut tx, true).await?;
    let who = actor(&mut tx, digest, None, true).await?;
    let current = records::read(&mut tx, who.principal).await?;
    let next = policy::revision(current.revision, expected)?;
    if current.locale != locale {
        write(&mut tx, who.principal, next, locale).await?;
        records::audit(&mut tx, &who, who.principal, next).await?;
    }
    let result = records::read(&mut tx, who.principal).await?;
    actor(&mut tx, digest, None, true).await?;
    tx.commit().await.map_err(storage)?;
    Ok(result)
}
async fn write(
    tx: &mut Tx<'_>,
    id: PrincipalId,
    revision: u64,
    locale: Option<Locale>,
) -> Result<(), Error> {
    let changed = sqlx::query("UPDATE principals SET preferred_locale=$2,revision=$3 WHERE id=$1")
        .bind(uuid(id.as_u128()))
        .bind(locale.map(crate::localization::tag))
        .bind(revision as i64)
        .execute(&mut **tx)
        .await
        .map_err(storage)?
        .rows_affected();
    if changed != 1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
