export function checkDatabaseVolumes(names, project) {
  if (
    names.includes(`${project}_database-data`) &&
    !names.includes(`${project}_percona-data`)
  )
    throw new Error(
      "Migrate the existing PostgreSQL volume before starting Percona; see docs/percona.md. The original volume is preserved.",
    );
}
