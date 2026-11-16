// Managed disposable fixture only, after all application/worker/operator jobs
// stop. The inserted sessions are expired and revoked on inactive principals;
// no usable handle, authorization code, access token or logout JWT is minted.
import {
  logoutSnapshotQuery,
  parseLogoutSnapshot,
} from "./logout-restore-snapshot.mjs";

export const logoutRestoreFixture = `BEGIN;
SET LOCAL statement_timeout='5s';
SET LOCAL lock_timeout='1s';
SELECT singleton FROM security_state WHERE singleton FOR UPDATE;
DO $fixture$
DECLARE
 client uuid; issuer_value text; actor uuid; old_revision bigint;
 pa uuid := gen_random_uuid(); pb uuid := gen_random_uuid();
 ca uuid := gen_random_uuid(); cb uuid := gen_random_uuid();
 sa uuid := gen_random_uuid(); sb uuid := gen_random_uuid(); sc uuid := gen_random_uuid();
 ra uuid := gen_random_uuid(); rb uuid := gen_random_uuid();
 da uuid := gen_random_uuid(); db uuid := gen_random_uuid();
 account_order bigint; account_time bigint;
 credential_order bigint; credential_time bigint; ia bigint; ib bigint;
 stamp bigint := floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 -- Reuse a real OIDC exchange's issuer/client, not a fabricated token.
 SELECT r.client_id,r.issuer,r.principal_id INTO STRICT client,issuer_value,actor
 FROM relying_party_sessions r
 WHERE EXISTS(SELECT 1 FROM token_audit a WHERE a.relying_party_session_id=r.sid AND a.event='code_redeemed')
 ORDER BY r.sid LIMIT 1;
 INSERT INTO principals(id,email,first_name,last_name,active) VALUES
  (pa,'restore-a-'||pa::text||'@example.invalid','Restore','Account',false),
  (pb,'restore-b-'||pb::text||'@example.invalid','Restore','Credential',false);
 INSERT INTO credentials(id,principal_id,kind) VALUES(ca,pa,'password'),(cb,pb,'password');
 -- Public policy-matched dummy verifier, never an authentication fixture.
 INSERT INTO password_credentials(credential_id,verifier)
 SELECT id,'$argon2id$v=19$m=65536,t=3,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
 FROM credentials WHERE id IN(ca,cb);
 INSERT INTO browser_sessions(digest,public_id,principal_id,credential_id,credential_epoch,created_ms,seen_ms,expires_ms,revoked)
 SELECT decode(repeat(replace(s::text,'-',''),2),'hex'),s,p,c,0,stamp-7200000,stamp-7200000,stamp-3600000,true
 FROM (VALUES(sa,pa,ca),(sb,pa,ca),(sc,pb,cb)) AS fixture(s,p,c);
 INSERT INTO relying_party_sessions(sid,issuer,session_id,principal_id,client_id,created_ms) VALUES
  (ra,issuer_value,sa,pa,client,stamp-7200000),
  (rb,issuer_value,sb,pa,client,stamp-7200000),
  (gen_random_uuid(),issuer_value,sc,pb,client,stamp-7200000);

 SELECT COALESCE(max(revision),0)+1 INTO old_revision FROM logout_destinations WHERE client_id=client;
 INSERT INTO logout_destinations(client_id,revision,uri) VALUES(client,old_revision,'https://restore.invalid/original');
 INSERT INTO logout_audit(client_id,actor_id,revision,event,occurred_ms)
 VALUES(client,actor,old_revision,'configured',stamp-120001);

 -- Account first for A, credential first for B. Actual revocation triggers
 -- create both immutable credential events and their queue roots at epoch 0.
 INSERT INTO logout_account_intents(principal_id,credential_epoch,created_ms)
 VALUES(pa,0,stamp-120000) RETURNING id,created_ms INTO account_order,account_time;
 UPDATE credentials SET revoked=true WHERE id=ca;
 UPDATE credentials SET revoked=true WHERE id=cb;
 SELECT ordering,created_ms INTO STRICT credential_order,credential_time FROM credential_logout_events WHERE credential_id=cb;
 INSERT INTO logout_account_intents(principal_id,credential_epoch,created_ms)
 VALUES(pb,0,floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
 UPDATE principals SET credential_epoch=1,revision=revision+1 WHERE id IN(pa,pb);

 -- Registration changes before lazy child expansion. Children must retain
 -- the earlier source order/time and original destination, not this revision.
 INSERT INTO logout_destinations(client_id,revision,uri) VALUES(client,old_revision+1,'https://restore.invalid/replacement');
 INSERT INTO logout_audit(client_id,actor_id,revision,event,occurred_ms)
 VALUES(client,actor,old_revision+1,'configured',stamp);
 INSERT INTO logout_intents(session_id,created_ms,destination_order,cursor,expanded)
 VALUES(sa,account_time,account_order,client,true) RETURNING id INTO ia;
 INSERT INTO logout_intents(session_id,created_ms,destination_order,cursor,expanded)
 VALUES(sb,account_time,account_order,client,true) RETURNING id INTO ib;
 INSERT INTO logout_intents(session_id,created_ms,destination_order)
 VALUES(sc,credential_time,credential_order);
 UPDATE logout_account_intents SET cursor=GREATEST(sa,sb),expanded=true WHERE id=account_order;
 UPDATE logout_credential_intents SET cursor=sc,expanded=true WHERE id=credential_order;
 INSERT INTO logout_deliveries(id,intent_id,sid,client_id,revision,state,attempts,created_ms,next_ms) VALUES
  (da,ia,ra,client,old_revision,'queued',2,account_time,account_time+120000),
  (db,ib,rb,client,old_revision,'delivered',1,account_time,account_time+2000);
 INSERT INTO logout_audit(client_id,delivery_id,revision,event,occurred_ms) VALUES
  (client,da,old_revision,'claimed',account_time+100),
  (client,da,old_revision,'retry',account_time+200),
  (client,da,old_revision,'claimed',account_time+60000),
  (client,db,old_revision,'claimed',account_time+1000),
  (client,db,old_revision,'delivered',account_time+2000);
END $fixture$;
COMMIT;`;

async function query(compose, stack, database, input) {
  if (!["darkhorse", "quarantine"].includes(database))
    throw new Error("Logout restore database rejected.");
  return compose(
    stack,
    [
      "exec",
      "-T",
      "postgres",
      "psql",
      "-X",
      "-U",
      "postgres",
      "-d",
      database,
      "-qAt",
      "-v",
      "ON_ERROR_STOP=1",
    ],
    { input, capture: true },
  );
}

export async function readLogoutSnapshot(compose, stack, database) {
  const result = await query(compose, stack, database, logoutSnapshotQuery);
  return parseLogoutSnapshot(result.stdout);
}

export async function assertQuarantinedLogoutHistory(compose, stack) {
  // Verify restored triggers, not just restored values. Every attempted write
  // must fail with the immutable-record trigger, and the transaction rolls back.
  await query(
    compose,
    stack,
    "quarantine",
    `BEGIN;
SET LOCAL statement_timeout='5s';
SET LOCAL lock_timeout='1s';
DO $check$
DECLARE statement text;
BEGIN
 FOREACH statement IN ARRAY ARRAY[
  'UPDATE credential_logout_events SET created_ms=created_ms WHERE ordering=(SELECT min(ordering) FROM credential_logout_events)',
  'DELETE FROM credential_logout_events WHERE ordering=(SELECT min(ordering) FROM credential_logout_events)',
  'UPDATE logout_destinations SET uri=uri WHERE ordering=(SELECT min(ordering) FROM logout_destinations)',
  'DELETE FROM logout_destinations WHERE ordering=(SELECT min(ordering) FROM logout_destinations)',
  'UPDATE logout_audit SET event=event WHERE id=(SELECT min(id) FROM logout_audit)',
  'DELETE FROM logout_audit WHERE id=(SELECT min(id) FROM logout_audit)'
 ] LOOP
  BEGIN
   EXECUTE statement;
   RAISE EXCEPTION 'restored immutable history accepted a mutation';
  EXCEPTION WHEN check_violation THEN
   IF SQLERRM <> 'immutable record' THEN RAISE; END IF;
  END;
 END LOOP;
END $check$;
ROLLBACK;`,
  );
}
