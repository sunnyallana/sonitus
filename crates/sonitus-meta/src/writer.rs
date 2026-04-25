//! Atomic writer for `.sonitus` files.
//!
//! Uses the same temp-file + fsync + rename pattern as `tempfile::persist`.
//! On success, the destination has been replaced atomically; a crash mid-
//! write leaves either the original or the new file untouched.

use crate::schema::LibraryMeta;
use crate::{MetaError, MetaResult};
use std::io::Write;
use std::path::Path;

/// Save a `.sonitus` file at `path`. Writes are atomic (temp + rename).
///
/// Updates `meta.updated_at` to `Utc::now()` automatically before serializing.
pub fn save(path: &Path, mut meta: LibraryMeta) -> MetaResult<()> {
    meta.meta.updated_at = chrono::Utc::now();
    let text = toml::to_string_pretty(&meta)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(text.as_bytes())?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| MetaError::Io(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_round_trip_preserves_sources() {
        use crate::schema::SourceDef;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("library.sonitus");

        let mut meta = LibraryMeta::default();
        meta.sources.push(SourceDef {
            id: "src_001".into(),
            name: "Local".into(),
            kind: "local".into(),
            enabled: true,
            path: Some("/home/user/Music".into()),
            root_folder: None,
            bucket: None,
            region: None,
            endpoint_url: None,
            host: None,
            share: None,
            base_path: None,
            url: None,
            tenant: None,
        });

        save(&path, meta.clone()).unwrap();
        let back = crate::reader::load(&path).unwrap();
        assert_eq!(back.sources.len(), 1);
        assert_eq!(back.sources[0].id, "src_001");
        assert_eq!(back.sources[0].path.as_deref(), Some("/home/user/Music"));
    }

    #[test]
    fn save_creates_parent_directory_if_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/dirs/library.sonitus");
        save(&path, LibraryMeta::default()).unwrap();
        assert!(path.exists());
    }
}
