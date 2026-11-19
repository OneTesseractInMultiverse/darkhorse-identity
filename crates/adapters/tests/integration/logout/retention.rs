use super::*;
use darkhorse_application::signing::SigningStore;
use std::time::Duration;

const QUEUES: [&str; 4] = [
    "logout_account_intents",
    "logout_credential_intents",
    "logout_deliveries",
    "logout_intents",
];
const HISTORY: [&str; 4] = [
    "credential_logout_events",
    "logout_audit",
    "relying_party_sessions",
    "logout_destinations",
];

async fn fixture() -> Database {
    let db = super::super::oidc::fixture().await;
    db.store.bind_provider(ISSUER, [12; 32]).await.unwrap();
    db.store
        .configure_logout(
            [1; 32],
            target(),
            config(0, Some("https://client.example/logout")),
        )
        .await
        .unwrap();
    db
}

// Insert historical facts with their final values. No immutable trigger is
// disabled and no existing event, association or destination is rewritten.
// Each principal has an empty session intent and a separate intent with a
// delivery child, so root deletion is exercised both with and without children.
async fn history(db: &Database, rows: i32, account_roots: bool, pending: bool) {
    history_at(db, rows, account_roots, pending, 1).await;
}

async fn history_at(db: &Database, rows: i32, account_roots: bool, pending: bool, created_ms: i64) {
    assert!((1..=102).contains(&rows));
    sqlx::query("INSERT INTO principals(id,email,first_name,last_name,credential_epoch,created_at) SELECT lpad(to_hex(n+1000),32,'0')::uuid,'retention-'||n||'@example.com','Historical','Fixture',$2,to_timestamp(0) FROM generate_series(1,$1) n")
        .bind(rows).bind(i64::from(account_roots)).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO credentials(id,principal_id,kind,revoked,created_at) SELECT lpad(to_hex(n+2000),32,'0')::uuid,lpad(to_hex(n+1000),32,'0')::uuid,'password',true,to_timestamp(0) FROM generate_series(1,$1) n")
        .bind(rows).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO password_credentials(credential_id,verifier) SELECT c.id,p.verifier FROM credentials c CROSS JOIN password_credentials p WHERE c.revoked AND p.credential_id='00000000-0000-0000-0000-000000000065'")
        .execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms,revoked) SELECT decode(lpad(to_hex(n+offset_id),64,'0'),'hex'),lpad(to_hex(n+offset_id),32,'0')::uuid,lpad(to_hex(n+1000),32,'0')::uuid,lpad(to_hex(n+2000),32,'0')::uuid,0,0,0,28800000,true FROM generate_series(1,$1) n CROSS JOIN (VALUES (3000),(4000)) offsets(offset_id)")
        .bind(rows).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO relying_party_sessions(issuer,session_id,principal_id,client_id,created_ms) SELECT $1,s.public_id,s.principal_id,c.id,0 FROM browser_sessions s CROSS JOIN oauth_clients c WHERE s.public_id BETWEEN $2 AND $3")
        .bind(ISSUER).bind(Uuid::from_u128(3001)).bind(Uuid::from_u128(3000 + rows as u128))
        .execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO credential_logout_events(credential_id,principal_id,credential_epoch,created_ms) SELECT id,principal_id,0,$1 FROM credentials WHERE revoked ORDER BY id")
        .bind(created_ms).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO logout_credential_intents(id,created_ms,expanded) SELECT ordering,created_ms,$1 FROM credential_logout_events")
        .bind(!pending).execute(&db.pool).await.unwrap();
    if account_roots {
        sqlx::query("INSERT INTO logout_account_intents(principal_id,credential_epoch,created_ms,expanded) SELECT principal_id,0,created_ms,$1 FROM credential_logout_events ORDER BY principal_id")
            .bind(!pending).execute(&db.pool).await.unwrap();
    }
    // Empty roots precede the child-bearing roots at the same retention age.
    sqlx::query("INSERT INTO logout_intents(session_id,created_ms,destination_order,expanded) SELECT s.public_id,e.created_ms,e.ordering,$1 OR EXISTS(SELECT 1 FROM relying_party_sessions r WHERE r.session_id=s.public_id) FROM browser_sessions s JOIN credential_logout_events e ON e.credential_id=s.credential_id ORDER BY s.public_id DESC")
        .bind(!pending).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO logout_deliveries(intent_id,sid,client_id,revision,state,created_ms,next_ms) SELECT i.id,r.sid,r.client_id,1,$1,i.created_ms,CASE WHEN $1='queued' THEN floor(extract(epoch FROM clock_timestamp())*1000)::bigint+86400000 ELSE i.created_ms+1 END FROM logout_intents i JOIN relying_party_sessions r ON r.session_id=i.session_id")
        .bind(if pending { "queued" } else { "delivered" }).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO logout_audit(client_id,delivery_id,revision,event,occurred_ms) SELECT client_id,id,revision,'delivered',next_ms FROM logout_deliveries WHERE state='delivered'")
        .execute(&db.pool).await.unwrap();
}

async fn snapshot(db: &Database, tables: &[&str]) -> Vec<String> {
    let mut facts = Vec::new();
    for table in tables {
        assert!(QUEUES.contains(table) || HISTORY.contains(table));
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT to_jsonb(r)::text FROM {table} r ORDER BY to_jsonb(r)::text"
        )))
        .fetch_all(&db.pool)
        .await
        .unwrap();
        facts.extend(rows.into_iter().map(|row| format!("{table}:{row}")));
    }
    facts
}

async fn counts(db: &Database) -> [i64; 4] {
    let mut counts = [0; 4];
    for (index, table) in QUEUES.iter().enumerate() {
        counts[index] =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&db.pool)
                .await
                .unwrap();
    }
    counts
}

async fn sweep(db: &Database) {
    // Bound a regression that waits on a locked row instead of skipping it.
    assert!(
        tokio::time::timeout(Duration::from_secs(5), db.store.claim())
            .await
            .expect("retention must not wait for the held row locks")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn retention_keeps_exactly_seven_days_and_removes_one_millisecond_older() {
    let db = fixture().await;
    let created = 100;
    history_at(&db, 1, true, false, created).await;
    let facts = snapshot(&db, &HISTORY).await;
    let queues = snapshot(&db, &QUEUES).await;
    let equality = created + darkhorse_domain::logout::RETENTION_MS as i64;
    for now in [equality - 1, equality] {
        darkhorse_adapters::postgres::cleanup_logout_at_for_test(&db.pool, now)
            .await
            .unwrap();
        assert_eq!(snapshot(&db, &QUEUES).await, queues);
        assert_eq!(snapshot(&db, &HISTORY).await, facts);
    }
    darkhorse_adapters::postgres::cleanup_logout_at_for_test(&db.pool, equality + 1)
        .await
        .unwrap();
    assert_eq!(counts(&db).await, [0, 0, 0, 0]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    db.store.close().await;
}

#[tokio::test]
async fn retention_claim_batches_each_queue_skips_locks_and_keeps_immutable_facts() {
    let db = fixture().await;
    history(&db, 102, true, false).await;
    let facts = snapshot(&db, &HISTORY).await;
    assert_eq!(counts(&db).await, [102, 102, 102, 204]);
    let mut locked = db.pool.begin().await.unwrap();
    for statement in [
        "SELECT id FROM logout_account_intents ORDER BY id LIMIT 1 FOR UPDATE",
        "SELECT id FROM logout_credential_intents ORDER BY id LIMIT 1 FOR UPDATE",
        "SELECT id FROM logout_deliveries ORDER BY id LIMIT 1 FOR UPDATE",
        "SELECT id FROM logout_intents ORDER BY id LIMIT 1 FOR UPDATE",
    ] {
        sqlx::query(statement).execute(&mut *locked).await.unwrap();
    }
    // 101 unlocked eligible account roots, credential roots and deliveries:
    // delete exactly 100, preserving the busy row and one unlocked remainder.
    // The session stage independently deletes 100 empty roots. Terminal child
    // removal makes more parents eligible without exceeding that stage's cap.
    sweep(&db).await;
    assert_eq!(counts(&db).await, [2, 2, 2, 104]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    sweep(&db).await;
    assert_eq!(counts(&db).await, [1, 1, 1, 4]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    locked.rollback().await.unwrap();
    sweep(&db).await;
    assert_eq!(counts(&db).await, [0, 0, 0, 0]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    db.store.close().await;
}

#[tokio::test]
async fn retention_keeps_old_unexpanded_roots_and_queued_children_with_their_parents() {
    let db = fixture().await;
    history(&db, 2, true, true).await;
    let facts = snapshot(&db, &HISTORY).await;
    let queued: Vec<(Uuid, i64)> =
        sqlx::query_as("SELECT id,intent_id FROM logout_deliveries ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    // Each production expansion consumes one root; the other old unexpanded
    // root in each category must survive cleanup without a protecting lock.
    sweep(&db).await;
    assert_eq!(counts(&db).await, [1, 1, 2, 3]);
    for table in [
        "logout_account_intents",
        "logout_credential_intents",
        "logout_intents",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE NOT expanded"
            )))
            .fetch_one(&db.pool)
            .await
            .unwrap(),
            1
        );
    }
    let retained: Vec<(Uuid, i64)> = sqlx::query_as("SELECT d.id,d.intent_id FROM logout_deliveries d JOIN logout_intents i ON i.id=d.intent_id WHERE d.state='queued' AND i.expanded ORDER BY d.id")
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(retained, queued);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    db.store.close().await;
}

#[tokio::test]
async fn retention_empty_session_roots_leave_the_101st_eligible_row_for_the_next_claim() {
    let db = fixture().await;
    history(&db, 50, true, false).await;
    sqlx::query("INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms,revoked) SELECT decode(lpad(to_hex(5001),64,'0'),'hex'),$1,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms,true FROM browser_sessions WHERE public_id=$2")
        .bind(Uuid::from_u128(5001)).bind(Uuid::from_u128(3001))
        .execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO logout_intents(session_id,created_ms,destination_order,expanded) SELECT s.public_id,e.created_ms,e.ordering,true FROM browser_sessions s JOIN credential_logout_events e ON e.credential_id=s.credential_id WHERE s.public_id=$1")
        .bind(Uuid::from_u128(5001))
        .execute(&db.pool).await.unwrap();
    assert_eq!(counts(&db).await, [50, 50, 50, 101]);
    sweep(&db).await;
    assert_eq!(counts(&db).await, [0, 0, 0, 1]);
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT session_id FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        Uuid::from_u128(5001)
    );
    sweep(&db).await;
    assert_eq!(counts(&db).await, [0, 0, 0, 0]);
    db.store.close().await;
}

#[tokio::test]
async fn retention_failure_in_last_cleanup_stage_rolls_back_all_four_queues() {
    let db = fixture().await;
    history(&db, 1, true, false).await;
    let queues = snapshot(&db, &QUEUES).await;
    let facts = snapshot(&db, &HISTORY).await;
    // The nontransactional sequence proves the failure was reached after the
    // other three deletion stages, rather than during an earlier statement.
    sqlx::raw_sql("CREATE SEQUENCE retention_cleanup_reached; CREATE FUNCTION reject_retention_cleanup() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF EXISTS(SELECT 1 FROM logout_account_intents) OR EXISTS(SELECT 1 FROM logout_credential_intents) OR EXISTS(SELECT 1 FROM logout_deliveries) THEN RAISE EXCEPTION 'earlier cleanup stages not reached'; END IF; PERFORM nextval('retention_cleanup_reached'); RAISE EXCEPTION 'injected retention failure'; END $$; CREATE TRIGGER reject_retention_cleanup BEFORE DELETE ON logout_intents FOR EACH ROW EXECUTE FUNCTION reject_retention_cleanup();")
        .execute(&db.pool).await.unwrap();
    assert!(db.store.claim().await.is_err());
    let reached: (i64, bool) =
        sqlx::query_as("SELECT last_value,is_called FROM retention_cleanup_reached")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(reached, (1, true));
    assert_eq!(snapshot(&db, &QUEUES).await, queues);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    sqlx::query("DROP TRIGGER reject_retention_cleanup ON logout_intents")
        .execute(&db.pool)
        .await
        .unwrap();
    sweep(&db).await;
    assert_eq!(counts(&db).await, [0, 0, 0, 0]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    db.store.close().await;
}

#[tokio::test]
async fn retention_keeps_credential_source_for_later_account_expansion_and_original_deadline() {
    let db = fixture().await;
    super::receiver::second_application(&db).await;
    let second = Target {
        application: ApplicationId::from_u128(17).unwrap(),
        client: ClientId::from_u128(33).unwrap(),
    };
    db.store
        .configure_logout(
            [1; 32],
            second,
            config(0, Some("https://second.example/logout")),
        )
        .await
        .unwrap();
    history(&db, 1, false, false).await;
    let facts = snapshot(&db, &HISTORY).await;
    let source: (i64, i64) =
        sqlx::query_as("SELECT ordering,created_ms FROM credential_logout_events")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    sweep(&db).await;
    assert_eq!(counts(&db).await, [0, 0, 0, 0]);
    assert_eq!(snapshot(&db, &HISTORY).await, facts);
    db.store
        .configure_logout(
            [1; 32],
            second,
            config(1, Some("https://replacement.example/logout")),
        )
        .await
        .unwrap();
    db.store
        .change(id(1001), 0, AccountAction::RevokeAll)
        .await
        .unwrap();
    let account: (i64, i64) = sqlx::query_as("SELECT id,created_ms FROM logout_account_intents")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(account.0 > source.0 && account.1 > source.1);
    assert!(source.1 + (darkhorse_domain::logout::MAX_AGE_MS as i64) < account.1);
    sweep(&db).await;
    let copied: (i64, i64) =
        sqlx::query_as("SELECT destination_order,created_ms FROM logout_intents")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(copied, source);
    let payloads: Vec<(Uuid, i64, String, i64, i16, String)> = sqlx::query_as("SELECT d.client_id,d.revision,v.uri,d.created_ms,d.attempts,d.state FROM logout_deliveries d JOIN logout_destinations v ON v.client_id=d.client_id AND v.revision=d.revision ORDER BY d.client_id")
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(payloads.len(), 2);
    for (client, revision, uri, created, attempts, _) in &payloads {
        assert_eq!((*revision, *created, *attempts), (1, source.1, 0));
        assert_eq!(
            uri,
            if client.as_u128() == 32 {
                "https://client.example/logout"
            } else {
                "https://second.example/logout"
            }
        );
    }
    // Claim ordering is by a random delivery ID for equal due times. Whichever
    // child was selected first is terminal; neither may acquire a fresh lease.
    assert!(payloads.iter().any(|row| row.5 != "queued"));
    sweep(&db).await;
    let outcomes: Vec<(Uuid, String)> = sqlx::query_as("SELECT client_id,event FROM logout_audit WHERE event IN ('failed','cancelled') ORDER BY client_id")
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(
        outcomes,
        vec![
            (Uuid::from_u128(32), "failed".to_owned()),
            (Uuid::from_u128(33), "cancelled".to_owned()),
        ]
    );
    // The unchanged endpoint fails at its original deadline; the replaced
    // endpoint is cancelled rather than retargeted to the new registration.
    let after = snapshot(&db, &HISTORY).await;
    assert!(facts.iter().all(|fact| after.contains(fact)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM logout_audit WHERE event='claimed'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    sweep(&db).await;
    assert_eq!(counts(&db).await, [1, 0, 0, 0]);
    db.store.close().await;
}
