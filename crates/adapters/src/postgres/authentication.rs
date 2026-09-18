use super::PostgresStore;
use darkhorse_application::authentication::{
    AuthError, AuthenticationStore, Candidate, SessionView,
};
use darkhorse_domain::{
    authentication::{ABSOLUTE_MS, SessionFacts, session_live},
    identity::{CredentialId, PrincipalId},
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

impl PostgresStore {
    /// Pin the nonsecret key fingerprint once. This does not reset counters.
    pub async fn bind_login_key(&self, key_digest: [u8; 32]) -> Result<(), AuthError> {
        sqlx::query("INSERT INTO login_budget_policy (key_digest) VALUES ($1) ON CONFLICT (singleton) DO NOTHING")
            .bind(key_digest.as_slice()).execute(&self.pool).await.map_err(unavailable)?;
        let matches: bool=sqlx::query_scalar("SELECT key_digest=$1 FROM login_budget_policy WHERE singleton AND NOT pg_is_in_recovery()")
            .bind(key_digest.as_slice()).fetch_one(&self.pool).await.map_err(unavailable)?;
        if !matches {
            return Err(AuthError::Unavailable);
        }
        Ok(())
    }
}

impl AuthenticationStore for PostgresStore {
    async fn candidate(&self, email_key: &str) -> Result<Option<Candidate>, AuthError> {
        let row = sqlx::query("SELECT p.id, p.active, p.credential_epoch, c.id AS credential, pc.verifier FROM principals p JOIN credentials c ON c.principal_id=p.id AND c.kind='password' AND NOT c.revoked JOIN password_credentials pc ON pc.credential_id=c.id WHERE p.email_key=$1 AND NOT pg_is_in_recovery()")
            .bind(email_key).fetch_optional(&self.pool).await.map_err(unavailable)?;
        row.map(candidate).transpose()
    }
    async fn establish(
        &self,
        verified: &Candidate,
        digest: [u8; 32],
        previous: Option<[u8; 32]>,
    ) -> Result<SessionView, AuthError> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let view = recheck(&mut tx, verified).await?;
        replace(&mut tx, previous).await?;
        insert(&mut tx, verified, digest).await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(view)
    }
    async fn session(&self, digest: [u8; 32]) -> Result<SessionView, AuthError> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        let row = sqlx::query("SELECT s.*, p.id, p.first_name, p.active, p.credential_epoch AS current_epoch, NOT c.revoked AS credential_live, floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms FROM browser_sessions s JOIN principals p ON p.id=s.principal_id JOIN credentials c ON c.id=s.credential_id AND c.principal_id=p.id WHERE s.digest=$1 AND NOT pg_is_in_recovery() FOR UPDATE OF s")
            .bind(digest.as_slice()).fetch_optional(&mut *tx).await.map_err(unavailable)?.ok_or(AuthError::Denied)?;
        let view = checked_session(&row)?;
        sqlx::query("UPDATE browser_sessions SET seen_ms=$2 WHERE digest=$1")
            .bind(digest.as_slice())
            .bind(row.try_get::<i64, _>("now_ms").map_err(unavailable)?)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(view)
    }
    async fn logout(&self, digest: [u8; 32]) -> Result<(), AuthError> {
        sqlx::query("UPDATE browser_sessions SET revoked=true WHERE digest=$1")
            .bind(digest.as_slice())
            .execute(&self.pool)
            .await
            .map_err(unavailable)?;
        Ok(())
    }
}

fn candidate(row: PgRow) -> Result<Candidate, AuthError> {
    Ok(Candidate {
        principal: principal(&row)?,
        credential: CredentialId::from_u128(
            row.try_get::<Uuid, _>("credential")
                .map_err(unavailable)?
                .as_u128(),
        )
        .map_err(unavailable)?,
        epoch: number(&row, "credential_epoch")?,
        active: row.try_get("active").map_err(unavailable)?,
        verifier: row.try_get("verifier").map_err(unavailable)?,
    })
}
async fn recheck(
    tx: &mut Transaction<'_, Postgres>,
    c: &Candidate,
) -> Result<SessionView, AuthError> {
    let row = sqlx::query("SELECT p.id, p.first_name FROM principals p JOIN credentials c ON c.principal_id=p.id AND c.kind='password' JOIN password_credentials pc ON pc.credential_id=c.id WHERE p.id=$1 AND c.id=$2 AND p.active AND NOT c.revoked AND p.credential_epoch=$3 AND pc.verifier=$4 AND NOT pg_is_in_recovery() FOR SHARE OF p,c,pc")
        .bind(Uuid::from_u128(c.principal.as_u128())).bind(Uuid::from_u128(c.credential.as_u128()))
        .bind(i64::try_from(c.epoch).map_err(unavailable)?).bind(&c.verifier)
        .fetch_optional(&mut **tx).await.map_err(unavailable)?.ok_or(AuthError::Denied)?;
    view(&row)
}
async fn replace(
    tx: &mut Transaction<'_, Postgres>,
    previous: Option<[u8; 32]>,
) -> Result<(), AuthError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let row = sqlx::query("SELECT revoked FROM browser_sessions WHERE digest=$1 FOR UPDATE")
        .bind(previous.as_slice())
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
    if row
        .as_ref()
        .is_some_and(|row| row.get::<bool, _>("revoked"))
    {
        return Err(AuthError::Denied);
    }
    sqlx::query("UPDATE browser_sessions SET revoked=true WHERE digest=$1")
        .bind(previous.as_slice())
        .execute(&mut **tx)
        .await
        .map_err(unavailable)?;
    Ok(())
}
async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    c: &Candidate,
    digest: [u8; 32],
) -> Result<(), AuthError> {
    sqlx::query("INSERT INTO browser_sessions (digest,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms) SELECT $1,$2,$3,$4,t,t,t+$5 FROM (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint t) n")
        .bind(digest.as_slice()).bind(Uuid::from_u128(c.principal.as_u128())).bind(Uuid::from_u128(c.credential.as_u128()))
        .bind(i64::try_from(c.epoch).map_err(unavailable)?).bind(ABSOLUTE_MS as i64).execute(&mut **tx).await.map_err(unavailable)?;
    Ok(())
}
fn checked_session(row: &PgRow) -> Result<SessionView, AuthError> {
    let facts = SessionFacts {
        active: row.try_get("active").map_err(unavailable)?,
        credential_live: row.try_get("credential_live").map_err(unavailable)?,
        revoked: row.try_get("revoked").map_err(unavailable)?,
        issued_epoch: number(row, "credential_epoch")?,
        current_epoch: number(row, "current_epoch")?,
        created_ms: number(row, "created_ms")?,
        seen_ms: number(row, "seen_ms")?,
        expires_ms: number(row, "expires_ms")?,
    };
    if !session_live(facts, number(row, "now_ms")?) {
        return Err(AuthError::Denied);
    }
    view(row)
}
fn view(row: &PgRow) -> Result<SessionView, AuthError> {
    Ok(SessionView {
        principal: principal(row)?,
        name: row.try_get("first_name").map_err(unavailable)?,
    })
}
fn principal(row: &PgRow) -> Result<PrincipalId, AuthError> {
    PrincipalId::from_u128(row.try_get::<Uuid, _>("id").map_err(unavailable)?.as_u128())
        .map_err(unavailable)
}
fn number(row: &PgRow, key: &str) -> Result<u64, AuthError> {
    row.try_get::<i64, _>(key)
        .map_err(unavailable)?
        .try_into()
        .map_err(unavailable)
}
fn unavailable<T>(_: T) -> AuthError {
    AuthError::Unavailable
}
