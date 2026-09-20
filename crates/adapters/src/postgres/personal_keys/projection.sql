WITH target AS MATERIALIZED (
 SELECT r.id AS resource_id, r.application_id, r.name AS resource_name,
        a.name AS application_name,a.active,p.active AS principal_active,p.credential_epoch
 FROM protected_resources r JOIN applications a ON a.id=r.application_id
 JOIN principals p ON p.id=$1 WHERE r.id=$2
), exposed AS MATERIALIZED (
 SELECT ARRAY(SELECT rc.capability_id FROM resource_capabilities rc
 JOIN capabilities c ON c.id=rc.capability_id AND NOT c.retired
 WHERE rc.resource_id=t.resource_id AND rc.application_id=t.application_id
 ORDER BY rc.capability_id LIMIT 257) AS capabilities FROM target t
)
SELECT t.*,e.capabilities,
 ARRAY(SELECT ROW(pr.role_id,ARRAY(SELECT capability_id FROM role_capabilities
  WHERE role_id=pr.role_id AND capability_id=ANY(e.capabilities) ORDER BY capability_id))
  FROM principal_roles pr WHERE pr.principal_id=$1 AND pr.application_id=t.application_id
  ORDER BY pr.role_id LIMIT 65) AS roles,
 ARRAY(SELECT id FROM capabilities WHERE id=ANY($3::uuid[]) ORDER BY id LIMIT 257) AS historical
FROM target t CROSS JOIN exposed e
