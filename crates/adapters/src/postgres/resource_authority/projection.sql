-- Read after the caller has acquired the primary security fence. Each collection
-- retains its overflow sentinel; the adapter rejects oversize authority.
WITH target AS MATERIALIZED (
    SELECT r.id AS resource_id, r.application_id, a.active,
           c.active AS client_active, p.active AS principal_active, p.credential_epoch
    FROM protected_resources r
    JOIN applications a ON a.id = r.application_id
    JOIN client_resources cr ON cr.resource_id = r.id AND cr.client_id = $1
    JOIN oauth_clients c ON c.id = cr.client_id
    JOIN principals p ON p.id = $2
    WHERE r.audience = $3
), exposed AS MATERIALIZED (
    SELECT ARRAY(
        SELECT rc.capability_id
        FROM resource_capabilities rc
        JOIN capabilities c ON c.id = rc.capability_id AND NOT c.retired
        WHERE rc.application_id = t.application_id AND rc.resource_id = t.resource_id
        ORDER BY rc.capability_id LIMIT 257
    ) AS capabilities FROM target t
)
SELECT t.*, e.capabilities,
    ARRAY(
        SELECT ROW(pr.role_id, ARRAY(
            SELECT capability_id FROM role_capabilities
            WHERE role_id = pr.role_id AND capability_id = ANY(e.capabilities)
            ORDER BY capability_id
        ))
        FROM principal_roles pr
        WHERE pr.principal_id = $2 AND pr.application_id = t.application_id
        ORDER BY pr.role_id LIMIT 65
    ) AS roles,
    ARRAY(
        SELECT ROW(s.id, ARRAY(
            SELECT capability_id FROM scope_capabilities
            WHERE scope_id = s.id AND capability_id = ANY(e.capabilities)
            ORDER BY capability_id
        ))
        FROM resource_scopes s
        JOIN client_scopes cs ON cs.scope_id = s.id AND cs.client_id = $1
        WHERE s.resource_id = t.resource_id AND s.name = ANY($4)
        ORDER BY s.id LIMIT 33
    ) AS scopes,
    ARRAY(
        SELECT id FROM capabilities WHERE id = ANY($5::uuid[])
        ORDER BY id LIMIT 257
    ) AS historical
FROM target t CROSS JOIN exposed e
