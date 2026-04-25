//! Library state — sources, scan progress, and counters.
//!
//! Cached locally so the UI can render without hitting the DB on every
//! frame. Updated by the orchestrator when scans complete.

use dioxus::prelude::*;
use sonitus_core::library::{Source, scanner::ScanProgress};

/// Snapshot of library-wide state.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    /// Currently-known sources.
    pub sources: Vec<Source>,
    /// Total tracks in the library.
    pub track_count: u64,
    /// Total albums.
    pub album_count: u64,
    /// Total artists.
    pub artist_count: u64,
    /// Total playlists.
    pub playlist_count: u64,
    /// In-progress scans, keyed by source ID.
    pub scan_progress: std::collections::HashMap<String, ScanProgress>,
    /// Last error encountered loading the library, if any.
    pub last_error: Option<String>,
    /// Bumped every time the orchestrator writes to the DB (track
    /// duration backfill, scan completion, etc.). Components that query
    /// the DB via `use_resource` should read this inside their closure
    /// to subscribe; the resource then re-runs after writes.
    pub version: u64,
}

/// Install a `Signal<LibraryState>` into the context.
pub fn install_library_state() {
    use_context_provider(|| Signal::new(LibraryState::default()));
}
