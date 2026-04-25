//! Library database facade.
//!
//! `Library` wraps a [`VaultDb`] and provides high-level operations that
//! span multiple tables. Single-table CRUD lives in [`crate::library::queries`].

use crate::crypto::VaultDb;
use crate::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

/// High-level library handle. Cheap to clone; backed by an `Arc<VaultDb>`.
#[derive(Clone)]
pub struct Library {
    db: Arc<VaultDb>,
}

impl Library {
    /// Construct a `Library` over an already-opened `VaultDb`.
    pub fn new(db: VaultDb) -> Self {
        Self { db: Arc::new(db) }
    }

    /// Construct from an `Arc<VaultDb>`. Used when sharing the same DB
    /// connection between the library and other components (download
    /// manager, audit logger).
    pub fn from_arc(db: Arc<VaultDb>) -> Self {
        Self { db }
    }

    /// Borrow the underlying `VaultDb`. Query modules need access to both
    /// the pool and the vault key (for encrypting/decrypting secret columns).
    pub fn vault(&self) -> &VaultDb {
        &self.db
    }

    /// Borrow the SQLx pool directly.
    pub fn pool(&self) -> &SqlitePool {
        self.db.pool()
    }

    /// Aggregate counts across the main tables. Used by the library home page.
    pub async fn summary(&self) -> Result<LibrarySummary> {
        let tracks: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks")
            .fetch_one(self.pool())
            .await?;
        let albums: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM albums")
            .fetch_one(self.pool())
            .await?;
        let artists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM artists")
            .fetch_one(self.pool())
            .await?;
        let playlists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM playlists")
            .fetch_one(self.pool())
            .await?;
        let sources: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sources WHERE enabled = 1")
            .fetch_one(self.pool())
            .await?;
        let total_duration: (Option<i64>,) =
            sqlx::query_as("SELECT COALESCE(SUM(duration_ms), 0) FROM tracks")
                .fetch_one(self.pool())
                .await?;
        Ok(LibrarySummary {
            tracks: tracks.0 as u64,
            albums: albums.0 as u64,
            artists: artists.0 as u64,
            playlists: playlists.0 as u64,
            enabled_sources: sources.0 as u64,
            total_duration_ms: total_duration.0.unwrap_or(0) as u64,
        })
    }
}

/// Summary stats shown on the library home page.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct LibrarySummary {
    /// Total tracks in the library.
    pub tracks: u64,
    /// Total albums.
    pub albums: u64,
    /// Total artists.
    pub artists: u64,
    /// Total playlists (manual + smart).
    pub playlists: u64,
    /// Sources currently enabled.
    pub enabled_sources: u64,
    /// Sum of all track durations in milliseconds.
    pub total_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn summary_of_empty_library_is_all_zeros() {
        let vault = VaultDb::open_in_memory().await.unwrap();
        let lib = Library::new(vault);
        let s = lib.summary().await.unwrap();
        assert_eq!(s.tracks, 0);
        assert_eq!(s.albums, 0);
        assert_eq!(s.artists, 0);
        assert_eq!(s.playlists, 0);
        assert_eq!(s.total_duration_ms, 0);
    }
}
