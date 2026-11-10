WITH targets AS MATERIALIZED (
    SELECT public_id,credential_id FROM browser_sessions
    WHERE principal_id=$1 AND credential_epoch=$2
        AND public_id>COALESCE($3::uuid,'00000000-0000-0000-0000-000000000000'::uuid)
    ORDER BY public_id LIMIT 100
), eligible AS MATERIALIZED (
    SELECT public_id,COALESCE(e.ordering,$4) AS ordering,COALESCE(e.created_ms,$5) AS created_ms
    FROM targets t LEFT JOIN credential_logout_events e
        ON e.credential_id=t.credential_id AND e.principal_id=$1 AND e.credential_epoch=$2 AND e.ordering<$4
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
