-- Operator role assignment is distinct from platform-administrator membership.
ALTER TABLE operator_access_catalog_audit
    DROP CONSTRAINT operator_access_catalog_audit_command_check,
    ADD CONSTRAINT operator_access_catalog_audit_command_check CHECK (command IN (
        'capability.create','capability.retire','role.create','capability.binding',
        'role.binding','role.capability','resource.capability','scope.capability',
        'principal.role'
    )),
    ADD COLUMN principal_expected_revision bigint CHECK(principal_expected_revision>=0),
    ADD COLUMN principal_resulting_revision bigint CHECK(principal_resulting_revision>=0),
    ADD CONSTRAINT operator_access_principal_revision_command CHECK (
        (command='principal.role') = (principal_expected_revision IS NOT NULL)
    ),
    ADD CONSTRAINT operator_access_principal_result CHECK (
        (command='principal.role' AND result IN ('changed','unchanged')) =
        (principal_resulting_revision IS NOT NULL)
    ),
    ADD CONSTRAINT operator_access_principal_revision_transition CHECK (
        principal_resulting_revision IS NULL OR
        (result='unchanged' AND principal_resulting_revision=principal_expected_revision) OR
        (result='changed' AND principal_expected_revision<9223372036854775807 AND
         principal_resulting_revision=principal_expected_revision+1)
    ),
    ADD CONSTRAINT operator_access_principal_target CHECK (
        command<>'principal.role' OR
        (application_id IS NOT NULL AND related_id IS NOT NULL AND requested_state IS NOT NULL)
    );

ALTER TABLE catalog_admin_audit
    DROP CONSTRAINT catalog_admin_audit_event_check,
    ADD CONSTRAINT catalog_admin_audit_event_check CHECK (event IN (
        'capability_created','capability_retired','role_created','capability_bound',
        'capability_unbound','role_bound','role_unbound','role_capability_granted',
        'role_capability_removed','resource_capability_exposed','resource_capability_removed',
        'scope_capability_included','scope_capability_removed',
        'principal_role_assigned','principal_role_removed'
    ));
