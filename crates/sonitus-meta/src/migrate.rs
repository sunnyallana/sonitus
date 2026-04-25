//! Forward-only migrations for the `.sonitus` schema.
//!
//! Each migration upgrades a `LibraryMeta` from version N to N+1. We run
//! all applicable migrations on load. Migrations must be idempotent and
//! free of side effects.

use crate::schema::LibraryMeta;

/// Apply every migration up to [`crate::CURRENT_SCHEMA_VERSION`].
pub fn up(mut meta: LibraryMeta) -> LibraryMeta {
    while meta.meta.schema_version < crate::CURRENT_SCHEMA_VERSION {
        let from = meta.meta.schema_version;
        meta = match from {
            0 => v0_to_v1(meta),
            // Future: 1 => v1_to_v2(meta),
            _ => break,
        };
        // After a migration, schema_version should have advanced.
        // If a migration neglects to bump, prevent infinite loops:
        if meta.meta.schema_version <= from {
            tracing::warn!(from, "migration did not bump schema_version; aborting upgrade chain");
            break;
        }
    }
    meta
}

/// Migration: schema_version 0 → 1. v0 had no `app` field; we synthesize one.
fn v0_to_v1(mut meta: LibraryMeta) -> LibraryMeta {
    if meta.meta.app.is_empty() {
        meta.meta.app = "sonitus".into();
    }
    meta.meta.schema_version = 1;
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_brings_v0_to_current() {
        let mut meta = LibraryMeta::default();
        meta.meta.schema_version = 0;
        meta.meta.app = String::new();
        let migrated = up(meta);
        assert_eq!(migrated.meta.schema_version, crate::CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.meta.app, "sonitus");
    }

    #[test]
    fn up_is_no_op_at_current_version() {
        let meta = LibraryMeta::default();
        let v_before = meta.meta.schema_version;
        let migrated = up(meta);
        assert_eq!(migrated.meta.schema_version, v_before);
    }
}
