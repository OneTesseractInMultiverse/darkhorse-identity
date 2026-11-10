WITH targets AS MATERIALIZED (
    SELECT public_id FROM browser_sessions
    -- A closed prefix range fixes credential, epoch and principal while seeking
    -- past the cursor. Matching prefix ordering keeps generic plans off the
    -- public-id-only index, which can otherwise scan unrelated sessions.
    WHERE (credential_id,credential_epoch,principal_id,public_id)>
        ($2,$3,$1,COALESCE($4::uuid,'00000000-0000-0000-0000-000000000000'::uuid))
        AND (credential_id,credential_epoch,principal_id,public_id)<=
        ($2,$3,$1,'ffffffff-ffff-ffff-ffff-ffffffffffff'::uuid)
    ORDER BY credential_id,credential_epoch,principal_id,public_id LIMIT 100
), eligible AS MATERIALIZED (
    SELECT public_id,COALESCE(a.id,$5) AS ordering,COALESCE(a.created_ms,$6) AS created_ms
    FROM targets t LEFT JOIN logout_account_intents a
        ON a.principal_id=$1 AND a.credential_epoch=$3 AND a.id<$5
    WHERE EXISTS(SELECT 1 FROM relying_party_sessions r WHERE r.session_id=t.public_id)
        AND NOT EXISTS(SELECT 1 FROM logout_intents i WHERE i.session_id=t.public_id)
), inserted AS (
    INSERT INTO logout_intents(session_id,created_ms,destination_order)
    SELECT public_id,created_ms,ordering FROM eligible RETURNING id
)
SELECT (SELECT count(*) FROM targets) AS targets,
    (SELECT public_id FROM targets ORDER BY public_id DESC LIMIT 1) AS cursor,
    (SELECT count(*) FROM eligible) AS eligible,
    (SELECT count(*) FROM inserted) AS inserted
