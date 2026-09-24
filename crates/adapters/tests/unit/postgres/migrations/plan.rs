use super::*;
use sqlx::{SqlSafeStr, migrate::MigrationType};
fn migration(version: i64) -> Migration {
    Migration::new(
        version,
        "fixture".into(),
        MigrationType::Simple,
        "SELECT 1".into_sql_str(),
        false,
    )
}
fn applied(m: &Migration) -> Applied {
    Applied {
        version: m.version,
        checksum: m.checksum.to_vec(),
        success: true,
    }
}
#[test]
fn accepts_only_a_matching_successful_prefix() {
    let manifest = vec![migration(1), migration(2)];
    assert_eq!(baseline(&manifest, &[]), Ok(0));
    assert_eq!(baseline(&manifest, &[applied(&manifest[0])]), Ok(1));
    assert_eq!(
        baseline(&manifest, &manifest.iter().map(applied).collect::<Vec<_>>()),
        Ok(2)
    );
    for records in [
        vec![applied(&manifest[1])],
        vec![applied(&migration(3))],
        vec![applied(&manifest[0]); 3],
    ] {
        assert_eq!(baseline(&manifest, &records), Err(Error::Incompatible));
    }
    let mut record = applied(&manifest[0]);
    record.success = false;
    assert_eq!(
        baseline(&manifest, &[record.clone()]),
        Err(Error::Incompatible)
    );
    record.success = true;
    record.checksum[0] ^= 1;
    assert_eq!(baseline(&manifest, &[record]), Err(Error::Incompatible));
}
#[test]
fn rejects_unbounded_nontransactional_reordered_or_down_manifests() {
    assert_eq!(baseline(&[], &[]), Err(Error::Incompatible));
    assert_eq!(
        baseline(&(1..=129).map(migration).collect::<Vec<_>>(), &[]),
        Err(Error::Incompatible)
    );
    assert_eq!(
        baseline(&(1..=128).map(migration).collect::<Vec<_>>(), &[]),
        Ok(0)
    );
    for manifest in [
        vec![migration(0)],
        vec![migration(2), migration(1)],
        vec![migration(1), migration(1)],
    ] {
        assert_eq!(baseline(&manifest, &[]), Err(Error::Incompatible));
    }
    let mut m = migration(1);
    m.no_tx = true;
    assert_eq!(baseline(&[m.clone()], &[]), Err(Error::Incompatible));
    m.no_tx = false;
    m.migration_type = MigrationType::ReversibleDown;
    assert_eq!(baseline(&[m.clone()], &[]), Err(Error::Incompatible));
    m.migration_type = MigrationType::Simple;
    m.checksum = vec![0].into();
    assert_eq!(baseline(&[m], &[]), Err(Error::Incompatible));
}
