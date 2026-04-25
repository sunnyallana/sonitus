//! Read a `.sonitus` file from disk.
//!
//! Verifies the schema version is in range, then parses TOML into
//! [`LibraryMeta`]. Forward-incompatible files (newer `schema_version`)
//! return an error rather than silently dropping fields.

use crate::schema::LibraryMeta;
use crate::{CURRENT_SCHEMA_VERSION, MetaError, MetaResult};
use std::path::Path;

/// Load a `.sonitus` file from `path`.
pub fn load(path: &Path) -> MetaResult<LibraryMeta> {
    let text = std::fs::read_to_string(path)?;
    let meta: LibraryMeta = toml::from_str(&text)?;
    if meta.meta.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(MetaError::UnsupportedVersion {
            found: meta.meta.schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    // Run forward migrations to bring older versions up to current.
    Ok(crate::migrate::up(meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::LibraryMeta;
    use tempfile::TempDir;

    #[test]
    fn load_round_trips_default_meta() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("library.sonitus");
        let meta = LibraryMeta::default();
        let text = toml::to_string_pretty(&meta).unwrap();
        std::fs::write(&path, text).unwrap();

        let back = load(&path).unwrap();
        assert_eq!(back.meta.app, "sonitus");
        assert_eq!(back.meta.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn load_rejects_too_new_schema() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("library.sonitus");
        let mut meta = LibraryMeta::default();
        meta.meta.schema_version = 999;
        std::fs::write(&path, toml::to_string_pretty(&meta).unwrap()).unwrap();
        let r = load(&path);
        assert!(matches!(r, Err(MetaError::UnsupportedVersion { .. })));
    }
}
