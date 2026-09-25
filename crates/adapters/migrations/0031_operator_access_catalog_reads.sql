-- Preserve the append-only catalog history while admitting explicit access reads.
ALTER TABLE operator_catalog_audit
 DROP CONSTRAINT operator_catalog_audit_command_check,
 DROP CONSTRAINT operator_catalog_audit_check,
 ADD CONSTRAINT operator_catalog_audit_command_check
  CHECK(command IN ('application.list','client.list','resource.list','scope.list','role.list','capability.list')),
 ADD CONSTRAINT operator_catalog_audit_target_check CHECK(
  (command='application.list' AND application_id IS NULL)
  OR (command IN ('client.list','resource.list','scope.list') AND application_id IS NOT NULL)
  OR command IN ('role.list','capability.list')
 ),
 ADD CONSTRAINT operator_catalog_audit_filter_check
  CHECK(active_filter IS NULL OR command IN ('application.list','client.list','capability.list'));
-- No authority expansion: runtime keeps SELECT/INSERT only on the existing ledger.
