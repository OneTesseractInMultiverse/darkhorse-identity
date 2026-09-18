use super::*;
use darkhorse_application::tokens::{Code, CodeStore, Opaque};
use darkhorse_domain::tokens::{self, Error as TokenError};
impl CodeStore for PostgresStore {
    async fn issue(
        &self,
        handle: [u8; 32],
        session: Option<[u8; 32]>,
        code: Opaque,
    ) -> Result<Code, TokenError> {
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        authority::lock(&mut tx).await.map_err(convert)?;
        let pending = records::pending(&mut tx, handle).await.map_err(convert)?;
        let catalog = records::catalog(&mut tx, pending.request.client)
            .await
            .map_err(convert)?;
        authority::unchanged(&pending, &catalog.policy).map_err(convert)?;
        tokens::profile(&pending.request.scopes, pending.request.resource.as_deref())?;
        let current = authority::session(&mut tx, session)
            .await
            .map_err(convert)?;
        let now = authority::now(&mut tx).await.map_err(convert)?;
        let consented = pending.approved
            || authority::consent(&mut tx, &pending.request, &catalog.policy, current)
                .await
                .map_err(convert)?;
        let mut request = pending.request.clone();
        request.prompt = continued_prompt(request.prompt, pending.approved);
        let state = interaction(
            &request,
            pending.created,
            now,
            pending.initial,
            pending.bound,
            current,
            consented,
        )
        .map_err(convert)?;
        ready(state)?;
        let session = current.ok_or(TokenError::InvalidGrant)?;
        insert(&mut tx, handle, &pending, session, code.digest, now).await?;
        writes::finish(&mut tx, handle).await.map_err(convert)?;
        super::super::tokens::audit(
            &mut tx,
            session.principal,
            request.client,
            "code_issued",
            now,
        )
        .await?;
        tx.commit().await.map_err(unavailable)?;
        Ok(Code {
            target: ReturnTo {
                uri: request.redirect,
                state: request.state,
            },
            value: code.value,
        })
    }
}
fn ready(state: Interaction) -> Result<(), TokenError> {
    if state != Interaction::Ready {
        return Err(TokenError::InvalidGrant);
    }
    Ok(())
}
async fn insert(
    tx: &mut Tx<'_>,
    handle: [u8; 32],
    p: &Pending,
    s: Session,
    digest: [u8; 32],
    now: u64,
) -> Result<(), TokenError> {
    sqlx::query("INSERT INTO authorization_codes(digest,request_digest,client_id,client_revision,application_revision,session_digest,principal_id,authenticated_ms,redirect_uri,challenge,nonce,created_ms,expires_ms,scopes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
  .bind(digest.as_slice()).bind(handle.as_slice()).bind(Uuid::from_u128(p.request.client.as_u128())).bind(p.client_revision as i64).bind(p.application_revision as i64).bind(s.digest.as_slice()).bind(Uuid::from_u128(s.principal.as_u128())).bind(s.authenticated_ms as i64).bind(&p.request.redirect).bind(p.request.challenge.as_slice()).bind(&p.request.nonce).bind(now as i64).bind(tokens::deadline(now,tokens::CODE_MS)? as i64).bind(&p.request.scopes).execute(&mut **tx).await.map_err(unavailable)?;
    Ok(())
}
fn convert(error: Error) -> TokenError {
    match error {
        Error::Unavailable => TokenError::Unavailable,
        _ => TokenError::InvalidGrant,
    }
}
fn unavailable<T>(_: T) -> TokenError {
    TokenError::Unavailable
}
