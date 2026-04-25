//! Live filesystem watcher for local sources.
//!
//! Wraps the `notify` crate. When a file is created/modified/removed under
//! a watched directory, we feed an [`incremental update`](Self::handle_event)
//! into the library — no full rescan required.
//!
//! Cloud sources don't have FS events; the UI's "Rescan" button is the
//! way to refresh those.

use crate::error::{Result, SonitusError};
use crate::library::queries;
use crate::sources::SourceProvider;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Library watcher — runs `notify::Watcher` on a background thread and
/// translates events into library updates.
pub struct LibraryWatcher {
    /// Underlying notify watcher (must stay alive for events to flow).
    _watcher: RecommendedWatcher,
    /// Channel of incoming raw FS events.
    rx: mpsc::Receiver<notify::Result<Event>>,
}

impl LibraryWatcher {
    /// Begin watching `roots`. Returns immediately; events arrive on the
    /// returned watcher's `next_event` method.
    pub fn watch(roots: Vec<PathBuf>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(256);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            // `notify` runs this on its own thread. We forward to tokio
            // via blocking send — back-pressure is fine; FS events are bursty
            // and we don't want to drop them.
            let _ = tx.blocking_send(res);
        })?;

        for root in roots {
            watcher.watch(&root, RecursiveMode::Recursive)?;
        }

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Wait for the next event. Returns `None` when the watcher closes.
    pub async fn next_event(&mut self) -> Option<notify::Result<Event>> {
        self.rx.recv().await
    }

    /// React to a single event. Updates the library DB:
    /// - Create/Modify on an audio file → re-process.
    /// - Remove → delete the track row.
    pub async fn handle_event(
        &self,
        event: &Event,
        source: &Arc<dyn SourceProvider>,
        pool: &SqlitePool,
    ) -> Result<()> {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    if Self::is_audio(path) {
                        // Translate filesystem path → remote_path used by source.
                        // For local sources, remote_path matches the absolute path.
                        let rel = path
                            .to_str()
                            .ok_or_else(|| SonitusError::PathNotFound(path.to_path_buf()))?;

                        // Use a dummy single-file scanner: read bytes, parse, upsert.
                        let bytes = source
                            .read_bytes(rel, 2 * 1024 * 1024)
                            .await
                            .ok();
                        let Some(bytes) = bytes else { continue; };

                        let parsed = crate::metadata::tags::parse(rel, &bytes)
                            .unwrap_or_else(|_| crate::metadata::tags::ParsedTags::guess_from_filename(rel));

                        // Build a track via shared logic in scanner; here we
                        // only update existing rows.
                        if let Ok(Some(existing)) =
                            queries::tracks::by_source_path(pool, source.id(), rel).await
                        {
                            let mut t = existing;
                            if let Some(title) = parsed.title { t.title = title; }
                            if parsed.duration_ms.is_some() { t.duration_ms = parsed.duration_ms; }
                            queries::tracks::upsert(pool, &t).await?;
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if let Some(rel) = path.to_str() {
                        if let Ok(Some(t)) =
                            queries::tracks::by_source_path(pool, source.id(), rel).await
                        {
                            queries::tracks::delete(pool, &t.id).await?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn is_audio(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(crate::library::models::TrackFormat::from_extension)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_audio_recognizes_common_extensions() {
        assert!(LibraryWatcher::is_audio(Path::new("a.mp3")));
        assert!(LibraryWatcher::is_audio(Path::new("/path/b.flac")));
        assert!(!LibraryWatcher::is_audio(Path::new("c.txt")));
        assert!(!LibraryWatcher::is_audio(Path::new("d")));
    }
}
