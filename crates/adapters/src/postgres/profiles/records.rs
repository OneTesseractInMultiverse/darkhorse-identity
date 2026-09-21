use super::*;
pub(super) async fn read(tx: &mut Tx<'_>, id: PrincipalId) -> Result<Profile, Error> {
    let row=sqlx::query("SELECT *, email_verified_ms IS NOT NULL AS email_verified FROM principals WHERE id=$1 FOR SHARE").bind(uuid(id.as_u128())).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(Error::NotFound)?;
    project(&row, id)
}
fn project(row: &PgRow, id: PrincipalId) -> Result<Profile, Error> {
    let code: Option<String> = row.try_get("calling_code").map_err(storage)?;
    let national: Option<String> = row.try_get("national_number").map_err(storage)?;
    let phone = match (code, national) {
        (None, None) => None,
        (Some(code), Some(national)) => Some(Phone::new(&code, &national).map_err(storage)?),
        _ => return Err(Error::Unavailable),
    };
    let fields = crate::profiles::prepare(Input {
        first_name: row.try_get("first_name").map_err(storage)?,
        last_name: row.try_get("last_name").map_err(storage)?,
        second_name: optional(row, "second_name")?,
        second_last_name: optional(row, "second_last_name")?,
        country: optional(row, "country")?,
        bio: optional(row, "bio")?,
        phone,
    })
    .map_err(storage)?;
    Ok(Profile {
        id,
        email: row.try_get("email").map_err(storage)?,
        active: row.try_get("active").map_err(storage)?,
        email_verified: row.try_get("email_verified").map_err(storage)?,
        revision: number(row, "revision")?,
        fields,
    })
}
fn optional(row: &PgRow, key: &str) -> Result<String, Error> {
    Ok(row
        .try_get::<Option<String>, _>(key)
        .map_err(storage)?
        .unwrap_or_default())
}
pub(super) async fn update(
    tx: &mut Tx<'_>,
    id: PrincipalId,
    revision: u64,
    f: &Fields,
) -> Result<(), Error> {
    sqlx::query("UPDATE principals SET first_name=$2,second_name=$3,last_name=$4,second_last_name=$5,country=$6,bio=$7,calling_code=$8,national_number=$9,revision=$10 WHERE id=$1")
 .bind(uuid(id.as_u128())).bind(f.first_name()).bind(f.second_name()).bind(f.last_name()).bind(f.second_last_name()).bind(f.country()).bind(f.bio()).bind(f.phone().map(Phone::calling_code)).bind(f.phone().map(Phone::national_number)).bind(revision as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn audit(
    tx: &mut Tx<'_>,
    actor: &Actor,
    target: PrincipalId,
    revision: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO profile_audit(actor_id,actor_session_id,target_id,target_revision,event,occurred_ms) VALUES($1,$2,$3,$4,'profile_updated',$5)").bind(uuid(actor.principal.as_u128())).bind(uuid(actor.session.as_u128())).bind(uuid(target.as_u128())).bind(revision as i64).bind(actor.now as i64).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
