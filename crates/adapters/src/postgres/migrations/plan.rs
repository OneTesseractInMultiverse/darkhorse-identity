use super::Error;
use sqlx::migrate::Migration;

pub(super) const LIMIT: usize = 128;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Applied {
    pub version: i64,
    pub checksum: Vec<u8>,
    pub success: bool,
}

pub(super) fn baseline(manifest: &[Migration], applied: &[Applied]) -> Result<usize, Error> {
    if manifest.is_empty()
        || manifest.len() > LIMIT
        || applied.len() > manifest.len()
        || manifest.iter().any(|m| {
            m.version <= 0
                || m.no_tx
                || m.migration_type.is_down_migration()
                || m.checksum.len() != 48
        })
        || manifest
            .windows(2)
            .any(|pair| pair[0].version >= pair[1].version)
        || applied.iter().zip(manifest).any(|(a, m)| {
            !a.success || a.version != m.version || a.checksum.as_slice() != m.checksum.as_ref()
        })
    {
        return Err(Error::Incompatible);
    }
    Ok(applied.len())
}

#[cfg(test)]
#[path = "../../../tests/unit/postgres/migrations/plan.rs"]
mod tests;
