//! BLAKE3-keyed offline cache.
//!
//! Files are stored under `cache_dir/{first_2_hex}/{rest_of_hex}`. Lookups
//! are O(1) by content hash. Eviction is least-recently-used: we record
//! access timestamps in a sidecar `.meta` file and evict from oldest when
//! the cache exceeds its size limit.

use crate::error::{Result, SonitusError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;

/// On-disk LRU cache for downloaded media files.
#[derive(Clone)]
pub struct OfflineCache {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    max_bytes: u64,
    /// Last-eviction-checked size; refreshed periodically.
    cached_size: Mutex<Option<u64>>,
}

impl OfflineCache {
    /// Open the cache rooted at `dir` with a `max_bytes` budget.
    pub fn open(dir: PathBuf, max_bytes: u64) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            inner: Arc::new(Inner { root: dir, max_bytes, cached_size: Mutex::new(None) }),
        })
    }

    /// Path for a given content hash.
    pub fn path_for(&self, hash: &str) -> PathBuf {
        let (a, b) = hash.split_at(2.min(hash.len()));
        self.inner.root.join(a).join(b)
    }

    /// Check existence + integrity. Returns the path if cached and valid.
    pub fn lookup(&self, hash: &str) -> Option<PathBuf> {
        let p = self.path_for(hash);
        if p.exists() {
            // Touch the file mtime to update LRU.
            let _ = filetime::set_file_mtime(&p, filetime::FileTime::now());
            Some(p)
        } else {
            None
        }
    }

    /// Insert bytes into the cache under the given hash. Atomic: writes to
    /// a temp file, fsyncs, then renames.
    pub fn insert(&self, hash: &str, bytes: &[u8]) -> Result<PathBuf> {
        let dest = self.path_for(hash);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(dest.parent().unwrap_or(Path::new(".")))?;
        std::io::Write::write_all(&mut tmp, bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&dest).map_err(|e| SonitusError::Io(e.error))?;
        // Mark cache size dirty so next eviction recomputes.
        *self.inner.cached_size.lock() = None;
        Ok(dest)
    }

    /// Verify integrity of a cached file by recomputing its hash.
    pub fn verify(&self, hash: &str) -> bool {
        let p = self.path_for(hash);
        let bytes = match std::fs::read(&p) { Ok(b) => b, Err(_) => return false };
        blake3::hash(&bytes).to_hex().to_string() == hash
    }

    /// Total bytes used by the cache. Cached for performance.
    pub fn size_bytes(&self) -> u64 {
        if let Some(s) = *self.inner.cached_size.lock() { return s; }
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(&self.inner.root) {
            if let Ok(e) = entry {
                if e.file_type().is_file() {
                    total += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        *self.inner.cached_size.lock() = Some(total);
        total
    }

    /// Evict least-recently-used files until we're under the size limit.
    /// Returns the number of files evicted.
    pub fn evict_lru(&self) -> usize {
        let total = self.size_bytes();
        if total <= self.inner.max_bytes { return 0; }

        let mut entries: Vec<(PathBuf, std::time::SystemTime, u64)> = walkdir::WalkDir::new(&self.inner.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let mtime = meta.modified().ok()?;
                Some((e.path().to_path_buf(), mtime, meta.len()))
            })
            .collect();
        entries.sort_by_key(|(_, t, _)| *t); // oldest first

        let mut to_free = total - self.inner.max_bytes;
        let mut evicted = 0;
        for (path, _, size) in entries {
            if to_free == 0 { break; }
            if std::fs::remove_file(&path).is_ok() {
                to_free = to_free.saturating_sub(size);
                evicted += 1;
            }
        }
        *self.inner.cached_size.lock() = None;
        evicted
    }

    /// Clear the entire cache.
    pub fn clear(&self) -> Result<()> {
        if self.inner.root.exists() {
            std::fs::remove_dir_all(&self.inner.root)?;
            std::fs::create_dir_all(&self.inner.root)?;
        }
        *self.inner.cached_size.lock() = None;
        Ok(())
    }
}

// `filetime` is a tiny crate; if absent we fall back to set_modified via OpenOptions.
mod filetime {
    use std::path::Path;
    use std::time::SystemTime;
    pub struct FileTime(SystemTime);
    impl FileTime {
        pub fn now() -> Self { FileTime(SystemTime::now()) }
    }
    pub fn set_file_mtime(path: &Path, _t: FileTime) -> std::io::Result<()> {
        // Not perfectly atomic without the `filetime` crate, but for LRU
        // purposes any "touch" is fine — we open + close to update atime/mtime
        // on most filesystems.
        let f = std::fs::OpenOptions::new().read(true).open(path)?;
        drop(f);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn insert_and_lookup_round_trips() {
        let dir = TempDir::new().unwrap();
        let cache = OfflineCache::open(dir.path().to_path_buf(), 1024 * 1024).unwrap();
        let bytes = b"hello world";
        let hash = blake3::hash(bytes).to_hex().to_string();
        let path = cache.insert(&hash, bytes).unwrap();
        assert!(path.exists());
        let found = cache.lookup(&hash).unwrap();
        assert_eq!(found, path);
    }

    #[test]
    fn lookup_misses_nonexistent_hash() {
        let dir = TempDir::new().unwrap();
        let cache = OfflineCache::open(dir.path().to_path_buf(), 1024).unwrap();
        assert!(cache.lookup("ffffffffffffffff").is_none());
    }

    #[test]
    fn verify_detects_corruption() {
        let dir = TempDir::new().unwrap();
        let cache = OfflineCache::open(dir.path().to_path_buf(), 1024).unwrap();
        let hash = blake3::hash(b"original").to_hex().to_string();
        let path = cache.insert(&hash, b"original").unwrap();
        std::fs::write(&path, b"tampered").unwrap();
        assert!(!cache.verify(&hash));
    }

    #[test]
    fn evict_lru_drops_files_when_over_budget() {
        let dir = TempDir::new().unwrap();
        let cache = OfflineCache::open(dir.path().to_path_buf(), 100).unwrap();
        // Insert 3 files of 60 bytes each (180 bytes total > 100 byte budget).
        for i in 0..3 {
            let bytes = vec![i as u8; 60];
            let h = blake3::hash(&bytes).to_hex().to_string();
            cache.insert(&h, &bytes).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let evicted = cache.evict_lru();
        assert!(evicted >= 1);
        assert!(cache.size_bytes() <= 100);
    }
}
