//! Download state — list of in-progress + completed downloads.

use dioxus::prelude::*;

/// One row in the downloads view.
#[derive(Debug, Clone)]
pub struct DownloadItem {
    /// Download row ID.
    pub id: String,
    /// Track being downloaded.
    pub track_id: String,
    /// Display title for the UI.
    pub track_title: String,
    /// Status string (`queued | downloading | done | failed | cancelled`).
    pub status: String,
    /// Progress 0.0..=1.0.
    pub progress: f64,
    /// Bytes received so far.
    pub bytes_done: u64,
    /// Total bytes if known.
    pub bytes_total: Option<u64>,
    /// Speed in bytes/sec.
    pub speed_bps: Option<u64>,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Install a `Signal<Vec<DownloadItem>>` into the context.
pub fn install_download_state() {
    use_context_provider(|| Signal::new(Vec::<DownloadItem>::new()));
}
