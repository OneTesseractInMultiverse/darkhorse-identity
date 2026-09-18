use super::PostgresStore;
use darkhorse_application::signing::*;
use darkhorse_domain::signing::{self, KeyError, KeyState, Phase};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
type Tx<'a> = Transaction<'a, Postgres>;
impl SigningStore for PostgresStore {
    async fn bind_provider(&self, issuer: &str, wrap_digest: [u8; 32]) -> Result<(), KeyError> {
        sqlx::query("INSERT INTO provider_state(issuer,wrap_digest,last_ms) VALUES($1,$2,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ON CONFLICT(singleton) DO NOTHING")
            .bind(issuer).bind(wrap_digest.as_slice()).execute(&self.pool).await.map_err(storage)?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_state WHERE issuer=$1 AND wrap_digest=$2 AND NOT pg_is_in_recovery())")
            .bind(issuer).bind(wrap_digest.as_slice()).fetch_one(&self.pool).await.map_err(storage)?;
        if !exists {
            return Err(KeyError::Conflict);
        }
        Ok(())
    }
    async fn inventory(&self, issuer: &str) -> Result<KeyInventory, KeyError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let (revision, _) = lock(&mut tx, issuer, None).await?;
        let rows=sqlx::query("SELECT kid,n,e,phase,created_ms,activated_ms,verify_until_ms FROM signing_keys WHERE phase<>'retired' ORDER BY created_ms,kid LIMIT 5")
            .fetch_all(&mut *tx).await.map_err(storage)?;
        if rows.len() > signing::MAX_PUBLISHED_KEYS {
            return Err(KeyError::Unavailable);
        }
        let keys = rows.iter().map(record).collect::<Result<_, _>>()?;
        tx.commit().await.map_err(storage)?;
        Ok(KeyInventory { revision, keys })
    }
    async fn stage(
        &self,
        issuer: &str,
        wrap_digest: [u8; 32],
        expected: u64,
        key: WrappedKey,
    ) -> Result<u64, KeyError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let (_, now) = lock(&mut tx, issuer, Some(expected)).await?;
        check_wrap(&mut tx, wrap_digest).await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM signing_keys WHERE phase<>'retired'")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        if count >= signing::MAX_PUBLISHED_KEYS as i64 {
            return Err(KeyError::Conflict);
        }
        advance(&mut tx, expected, now).await?;
        sqlx::query("INSERT INTO signing_keys(kid,n,e,nonce,ciphertext,phase,created_ms) VALUES($1,$2,$3,$4,$5,'staged',$6)")
            .bind(&key.public.kid).bind(&key.public.n).bind(&key.public.e).bind(key.nonce.as_slice()).bind(&key.ciphertext).bind(integer(now)?).execute(&mut *tx).await.map_err(constraint)?;
        audit(&mut tx, "key_staged", &key.public.kid, expected, now).await?;
        tx.commit().await.map_err(storage)?;
        next(expected)
    }
    async fn activate(&self, issuer: &str, kid: &str, expected: u64) -> Result<u64, KeyError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let (_, now) = lock(&mut tx, issuer, Some(expected)).await?;
        let key = load(&mut tx, kid).await?;
        signing::activate(key.state, now)?;
        advance(&mut tx, expected, now).await?;
        sqlx::query(
            "UPDATE signing_keys SET phase='retiring',verify_until_ms=$1 WHERE phase='active'",
        )
        .bind(integer(signing::overlap_end(now)?)?)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        sqlx::query("UPDATE signing_keys SET phase='active',activated_ms=$2 WHERE kid=$1")
            .bind(kid)
            .bind(integer(now)?)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        audit(&mut tx, "key_activated", kid, expected, now).await?;
        tx.commit().await.map_err(storage)?;
        next(expected)
    }
    async fn retire(&self, issuer: &str, kid: &str, expected: u64) -> Result<u64, KeyError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let (_, now) = lock(&mut tx, issuer, Some(expected)).await?;
        let key = load(&mut tx, kid).await?;
        signing::retire(key.state, now)?;
        advance(&mut tx, expected, now).await?;
        sqlx::query("UPDATE signing_keys SET phase='retired',ciphertext=NULL WHERE kid=$1")
            .bind(kid)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        audit(&mut tx, "key_retired", kid, expected, now).await?;
        tx.commit().await.map_err(storage)?;
        next(expected)
    }
    async fn published(&self, issuer: &str) -> Result<Vec<PublicKey>, KeyError> {
        let rows=sqlx::query("SELECT k.kid,k.n,k.e,k.phase,k.created_ms,k.activated_ms,k.verify_until_ms,floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms,s.last_ms FROM provider_state s CROSS JOIN signing_keys k WHERE s.issuer=$1 AND k.phase<>'retired' AND NOT pg_is_in_recovery() ORDER BY k.created_ms,k.kid LIMIT 5")
            .bind(issuer).fetch_all(&self.pool).await.map_err(storage)?;
        published(&rows)
    }
}
async fn lock(
    tx: &mut Tx<'_>,
    issuer: &str,
    expected: Option<u64>,
) -> Result<(u64, u64), KeyError> {
    let row=sqlx::query("SELECT revision,last_ms FROM provider_state WHERE issuer=$1 AND NOT pg_is_in_recovery() FOR UPDATE")
        .bind(issuer).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(KeyError::NotFound)?;
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
    let revision = number(&row, "revision")?;
    if expected.is_some_and(|expected| expected != revision) {
        return Err(KeyError::Conflict);
    }
    let now = u64::try_from(now).map_err(storage)?;
    if now < number(&row, "last_ms")? {
        return Err(KeyError::Unavailable);
    }
    Ok((revision, now))
}
async fn check_wrap(tx: &mut Tx<'_>, digest: [u8; 32]) -> Result<(), KeyError> {
    let matches: bool =
        sqlx::query_scalar("SELECT wrap_digest=$1 FROM provider_state WHERE singleton")
            .bind(digest.as_slice())
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
    if !matches {
        return Err(KeyError::Conflict);
    }
    Ok(())
}
async fn load(tx: &mut Tx<'_>, kid: &str) -> Result<KeyRecord, KeyError> {
    let row=sqlx::query("SELECT kid,n,e,phase,created_ms,activated_ms,verify_until_ms FROM signing_keys WHERE kid=$1")
        .bind(kid).fetch_optional(&mut **tx).await.map_err(storage)?.ok_or(KeyError::NotFound)?;
    record(&row)
}
async fn advance(tx: &mut Tx<'_>, revision: u64, now: u64) -> Result<(), KeyError> {
    sqlx::query("UPDATE provider_state SET revision=$1,last_ms=$2 WHERE singleton")
        .bind(integer(next(revision)?)?)
        .bind(integer(now)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
async fn audit(
    tx: &mut Tx<'_>,
    event: &str,
    kid: &str,
    revision: u64,
    now: u64,
) -> Result<(), KeyError> {
    sqlx::query("INSERT INTO provider_audit(event,kid,revision,occurred_ms) VALUES($1,$2,$3,$4)")
        .bind(event)
        .bind(kid)
        .bind(integer(next(revision)?)?)
        .bind(integer(now)?)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    Ok(())
}
fn record(row: &PgRow) -> Result<KeyRecord, KeyError> {
    let phase = match row.try_get::<String, _>("phase").map_err(storage)?.as_str() {
        "staged" => Phase::Staged,
        "active" => Phase::Active,
        "retiring" => Phase::Retiring,
        "retired" => Phase::Retired,
        _ => return Err(KeyError::Unavailable),
    };
    Ok(KeyRecord {
        public: public(row)?,
        state: KeyState {
            phase,
            created_ms: number(row, "created_ms")?,
            activated_ms: optional(row, "activated_ms")?,
            verify_until_ms: optional(row, "verify_until_ms")?,
        },
    })
}
fn published(rows: &[PgRow]) -> Result<Vec<PublicKey>, KeyError> {
    if rows.len() > signing::MAX_PUBLISHED_KEYS {
        return Err(KeyError::Unavailable);
    }
    let mut keys = Vec::new();
    for row in rows {
        let now = number(row, "now_ms")?;
        if now < number(row, "last_ms")? {
            return Err(KeyError::Unavailable);
        }
        let record = record(row)?;
        if signing::published(record.state, now) {
            keys.push(record.public);
        }
    }
    Ok(keys)
}
fn public(row: &PgRow) -> Result<PublicKey, KeyError> {
    Ok(PublicKey {
        kid: row.try_get("kid").map_err(storage)?,
        n: row.try_get("n").map_err(storage)?,
        e: row.try_get("e").map_err(storage)?,
    })
}
fn number(row: &PgRow, key: &str) -> Result<u64, KeyError> {
    row.try_get::<i64, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(storage)
}
fn optional(row: &PgRow, key: &str) -> Result<Option<u64>, KeyError> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(storage)?
        .map(|n| n.try_into().map_err(storage))
        .transpose()
}
fn integer(n: u64) -> Result<i64, KeyError> {
    n.try_into().map_err(storage)
}
fn next(n: u64) -> Result<u64, KeyError> {
    n.checked_add(1)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(KeyError::Conflict)
}
fn storage<T>(_: T) -> KeyError {
    KeyError::Unavailable
}
fn constraint(error: sqlx::Error) -> KeyError {
    if error
        .as_database_error()
        .is_some_and(|e| e.code().as_deref() == Some("23505"))
    {
        KeyError::Conflict
    } else {
        KeyError::Unavailable
    }
}
