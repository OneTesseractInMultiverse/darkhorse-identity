use super::*;
use darkhorse_domain::identity::RelyingPartySessionId;

/// Caller holds the security fence and has rechecked the code and live session.
/// Concurrent redemptions insert once; the following statement sees the winner.
pub(super) async fn bind(
    tx: &mut Tx<'_>,
    code: &CodeRecord,
    issuer: &str,
    now: u64,
) -> Result<RelyingPartySessionId, Error> {
    sqlx::query("INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT p.issuer,s.public_id,s.principal_id,$3,$4 FROM browser_sessions s JOIN provider_state p ON p.issuer=$2 WHERE s.digest=$1 AND s.principal_id=$5 ON CONFLICT(issuer,session_id,client_id) DO NOTHING")
        .bind(code.session.as_slice()).bind(issuer).bind(Uuid::from_u128(code.client.as_u128()))
        .bind(now as i64).bind(Uuid::from_u128(code.principal.as_u128()))
        .execute(&mut **tx).await.map_err(storage)?;
    let now = authority::now(tx).await.map_err(storage)?;
    let sid: Uuid = sqlx::query_scalar("SELECT r.sid FROM relying_party_sessions r JOIN browser_sessions s ON s.public_id=r.session_id WHERE s.digest=$1 AND r.issuer=$2 AND r.client_id=$3 AND r.principal_id=$4 AND r.created_ms<=$5")
        .bind(code.session.as_slice()).bind(issuer).bind(Uuid::from_u128(code.client.as_u128()))
        .bind(Uuid::from_u128(code.principal.as_u128())).bind(now as i64)
        .fetch_one(&mut **tx).await.map_err(storage)?;
    RelyingPartySessionId::from_u128(sid.as_u128()).map_err(storage)
}

pub(super) async fn audit(
    tx: &mut Tx<'_>,
    code: &CodeRecord,
    session: RelyingPartySessionId,
    now: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO token_audit(principal_id,client_id,event,occurred_ms,relying_party_session_id) VALUES($1,$2,'code_redeemed',$3,$4)")
        .bind(Uuid::from_u128(code.principal.as_u128()))
        .bind(Uuid::from_u128(code.client.as_u128())).bind(now as i64)
        .bind(Uuid::from_u128(session.as_u128()))
        .execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
