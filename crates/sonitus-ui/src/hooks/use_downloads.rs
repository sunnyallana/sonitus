//! `use_downloads()` — read download queue state.

use crate::state::download_state::DownloadItem;
use dioxus::prelude::*;

/// Handle returned by [`use_downloads`].
#[derive(Clone, Copy)]
pub struct DownloadsHandle {
    state: Signal<Vec<DownloadItem>>,
}

impl DownloadsHandle {
    /// Snapshot of all items.
    pub fn read(&self) -> Vec<DownloadItem> {
        self.state.read().clone()
    }

    /// Number of in-progress downloads.
    pub fn active_count(&self) -> usize {
        self.state
            .read()
            .iter()
            .filter(|d| d.status == "downloading" || d.status == "queued")
            .count()
    }
}

/// Hook to access downloads state.
pub fn use_downloads() -> DownloadsHandle {
    let state = use_context::<Signal<Vec<DownloadItem>>>();
    DownloadsHandle { state }
}
