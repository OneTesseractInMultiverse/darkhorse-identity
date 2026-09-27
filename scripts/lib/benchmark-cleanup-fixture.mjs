const maxRowsPerState = 20_000;
const applicationIdPattern =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function lifecycleFixtureStatements(
  applicationId,
  { expiredRows, liveRows },
) {
  if (!applicationIdPattern.test(applicationId))
    throw new Error("Lifecycle benchmark application identifier is invalid.");
  for (const rows of [expiredRows, liveRows])
    if (!Number.isInteger(rows) || rows < 1 || rows > maxRowsPerState)
      throw new Error("Lifecycle benchmark population is outside safe bounds.");

  const seed = `WITH clock AS (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS ms), synthetic AS (
    SELECT n AS n,clock.ms-600000 AS created_ms,clock.ms-300000 AS expires_ms FROM generate_series(1,${expiredRows}) AS expired(n) CROSS JOIN clock
    UNION ALL
    SELECT n+${expiredRows},clock.ms,clock.ms+300000 FROM generate_series(1,${liveRows}) AS live(n) CROSS JOIN clock
  )
  INSERT INTO authorization_requests(digest,client_id,client_revision,application_revision,redirect_uri,challenge,scopes,prompt,created_ms,expires_ms)
  SELECT decode('d4c1dead00000000'||lpad(to_hex(synthetic.n),48,'0'),'hex'),client.id,0,0,'https://client.example/callback?fixed=1',decode(repeat('07',32),'hex'),ARRAY['openid'],'default',synthetic.created_ms,synthetic.expires_ms
  FROM synthetic CROSS JOIN LATERAL (SELECT id FROM oauth_clients WHERE application_id='${applicationId}' ORDER BY id LIMIT 1) AS client`;
  const counts = `WITH clock AS (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint AS ms)
  SELECT json_build_object(
    'expired',count(*) FILTER (WHERE request.expires_ms<=clock.ms),
    'live',count(*) FILTER (WHERE request.expires_ms>clock.ms)
  )::text
  FROM authorization_requests AS request CROSS JOIN clock
  WHERE substring(request.digest FROM 1 FOR 8)=decode('d4c1dead00000000','hex')
    AND request.client_id IN (SELECT id FROM oauth_clients WHERE application_id='${applicationId}')`;
  return { seed, counts };
}
