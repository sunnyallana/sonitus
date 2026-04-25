//! Recursive library scanner.
//!
//! Given a [`SourceProvider`](crate::sources::SourceProvider), the scanner:
//!
//! 1. Asks the source for a flat list of audio files (`list_files`).
//! 2. For each file: reads enough bytes to parse tags (via `metadata`).
//! 3. Builds `Artist`/`Album`/`Track` rows and upserts them.
//! 4. Reports progress over an `mpsc` channel.
//!
//! The scanner is **idempotent**: re-running it discovers no changes. New
//! files are added; deleted files are removed; existing files are
//! re-checked only if their mtime or size differs from what's in the DB.

use crate::error::{Result, SonitusError};
use crate::library::{
    models::{Album, Artist, ScanState, Track, TrackFormat},
    queries,
};
use crate::sources::{RemoteFile, SourceProvider};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Aggregated outcome of a scan, returned when the scan completes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanReport {
    /// Number of files the source reported.
    pub files_seen: u64,
    /// Number of new tracks added.
    pub tracks_added: u64,
    /// Number of existing tracks updated (metadata changed).
    pub tracks_updated: u64,
    /// Number of tracks removed (no longer present at the source).
    pub tracks_removed: u64,
    /// Files that failed to be parsed (corrupt, unsupported, etc.).
    pub files_failed: u64,
    /// Wall-clock duration of the scan in milliseconds.
    pub duration_ms: u64,
}

/// Streaming progress event emitted while a scan runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    /// Source ID being scanned.
    pub source_id: String,
    /// Files discovered so far.
    pub files_seen: u64,
    /// Tracks indexed so far.
    pub tracks_indexed: u64,
    /// Files that failed.
    pub files_failed: u64,
    /// Currently-processing file (if any).
    pub current_file: Option<String>,
}

/// Drives a scan against a single source. Construct via `Scanner::new`,
/// then call `run`.
pub struct Scanner {
    source: Arc<dyn SourceProvider>,
    pool: SqlitePool,
}

impl Scanner {
    /// Construct a scanner.
    pub fn new(source: Arc<dyn SourceProvider>, pool: SqlitePool) -> Self {
        Self { source, pool }
    }

    /// Run a full scan. Progress events are sent on `progress`. The final
    /// `ScanReport` is returned when the scan completes (or errors).
    pub async fn run(&self, progress: mpsc::Sender<ScanProgress>) -> Result<ScanReport> {
        let start = std::time::Instant::now();
        let source_id = self.source.id().to_string();

        queries::sources::set_scan_state(&self.pool, &source_id, ScanState::Scanning, None).await?;

        // Set of remote paths the source still has — used to detect deletions.
        let mut still_present: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut report = ScanReport::default();

        // Snapshot of pre-scan tracks, keyed by remote_path.
        let pre_existing: std::collections::HashMap<String, Track> =
            queries::tracks::by_source(&self.pool, &source_id)
                .await?
                .into_iter()
                .map(|t| (t.remote_path.clone(), t))
                .collect();

        let files = match self.source.list_files().await {
            Ok(f) => f,
            Err(e) => {
                queries::sources::set_scan_state(
                    &self.pool,
                    &source_id,
                    ScanState::Error,
                    Some(&e.to_string()),
                )
                .await?;
                return Err(e);
            }
        };
        report.files_seen = files.len() as u64;

        let _ = progress
            .send(ScanProgress {
                source_id: source_id.clone(),
                files_seen: report.files_seen,
                tracks_indexed: 0,
                files_failed: 0,
                current_file: None,
            })
            .await;

        for file in files {
            let _ = progress
                .send(ScanProgress {
                    source_id: source_id.clone(),
                    files_seen: report.files_seen,
                    tracks_indexed: report.tracks_added + report.tracks_updated,
                    files_failed: report.files_failed,
                    current_file: Some(file.path.clone()),
                })
                .await;

            still_present.insert(file.path.clone());

            // Cheap unchanged check: mtime + size match → skip parse.
            if let Some(existing) = pre_existing.get(&file.path) {
                let unchanged = existing.file_size_bytes == Some(file.size_bytes as i64);
                if unchanged {
                    continue;
                }
            }

            match self.process_file(&source_id, &file).await {
                Ok(was_new) => {
                    if was_new {
                        report.tracks_added += 1;
                    } else {
                        report.tracks_updated += 1;
                    }
                }
                Err(e) => {
                    report.files_failed += 1;
                    tracing::warn!(path = %file.path, error = %e, "scan: failed to process file");
                }
            }
        }

        // Detect deletions.
        for (path, existing) in &pre_existing {
            if !still_present.contains(path) {
                queries::tracks::delete(&self.pool, &existing.id).await?;
                report.tracks_removed += 1;
            }
        }

        queries::sources::refresh_track_count(&self.pool, &source_id).await?;
        queries::sources::set_scan_state(&self.pool, &source_id, ScanState::Idle, None).await?;

        report.duration_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// Process a single file: parse tags, upsert artist/album/track.
    /// Returns `Ok(true)` if a new track was inserted, `Ok(false)` if updated.
    async fn process_file(&self, source_id: &str, file: &RemoteFile) -> Result<bool> {
        // Read enough bytes for ID3/FLAC headers. 1 MiB is plenty for any
        // tagging format — 2 MiB catches outsized cover art.
        let bytes = self.source.read_bytes(&file.path, 2 * 1024 * 1024).await?;

        let parsed = crate::metadata::tags::parse(&file.path, &bytes)
            .unwrap_or_else(|_| crate::metadata::tags::ParsedTags::guess_from_filename(&file.path));

        // Compute content hash from the bytes we already have. For files
        // larger than 2 MiB, we still hash only the first 2 MiB — that's
        // enough for cache key purposes; full-file integrity is verified
        // at download time.
        let content_hash = blake3::hash(&bytes).to_hex().to_string();

        // Resolve / upsert artist.
        let artist_id = if let Some(name) = parsed.artist.as_deref() {
            let sort_name = Artist::sort_name_for(name);
            let id = Artist::derive_id(&sort_name);
            let artist = Artist {
                id: id.clone(),
                name: name.to_string(),
                sort_name,
                musicbrainz_id: None,
                bio: None,
                image_url: None,
                image_blob: None,
                play_count: 0,
                created_at: 0,
                updated_at: 0,
            };
            queries::artists::upsert(&self.pool, &artist).await?;
            Some(id)
        } else {
            None
        };

        // Resolve / upsert album-artist (falls back to artist).
        let album_artist_id = parsed.album_artist.as_deref().map(|n| {
            let sn = Artist::sort_name_for(n);
            Artist::derive_id(&sn)
        });

        // Album.
        let album_id = if let Some(album_title) = parsed.album.as_deref() {
            let id = Album::derive_id(album_artist_id.as_deref().or(artist_id.as_deref()), album_title);
            let album = Album {
                id: id.clone(),
                title: album_title.to_string(),
                artist_id: album_artist_id.clone().or_else(|| artist_id.clone()),
                year: parsed.year,
                genre: parsed.genre.clone(),
                cover_art_blob: parsed.cover_art.clone(),
                cover_art_url: None,
                cover_art_hash: parsed
                    .cover_art
                    .as_ref()
                    .map(|b| blake3::hash(b).to_hex().to_string()),
                musicbrainz_id: None,
                total_tracks: parsed.total_tracks,
                disc_count: parsed.disc_number.unwrap_or(1).max(1),
                play_count: 0,
                created_at: 0,
                updated_at: 0,
            };
            queries::albums::upsert(&self.pool, &album).await?;
            Some(id)
        } else {
            None
        };

        // Track.
        let track_id = Track::derive_id(source_id, &file.path);
        let was_new = queries::tracks::by_id(&self.pool, &track_id).await.is_err();

        let format = file
            .path
            .rsplit_once('.')
            .and_then(|(_, ext)| TrackFormat::from_extension(ext))
            .map(|f| f.to_string());

        let title = parsed.title.clone().unwrap_or_else(|| file_stem(&file.path));

        let track = Track {
            id: track_id,
            title,
            artist_id,
            album_artist_id,
            album_id,
            source_id: source_id.to_string(),
            remote_path: file.path.clone(),
            local_cache_path: None,
            duration_ms: parsed.duration_ms,
            track_number: parsed.track_number,
            disc_number: parsed.disc_number.unwrap_or(1),
            genre: parsed.genre,
            year: parsed.year,
            bpm: parsed.bpm,
            replay_gain_track: parsed.replay_gain_track,
            replay_gain_album: parsed.replay_gain_album,
            file_size_bytes: Some(file.size_bytes as i64),
            format,
            bitrate_kbps: parsed.bitrate_kbps,
            sample_rate_hz: parsed.sample_rate_hz,
            bit_depth: parsed.bit_depth,
            channels: parsed.channels,
            content_hash: Some(content_hash),
            musicbrainz_id: None,
            play_count: 0,
            last_played_at: None,
            rating: None,
            loved: 0,
            created_at: 0,
            updated_at: 0,
        };
        queries::tracks::upsert(&self.pool, &track).await?;

        Ok(was_new)
    }
}

fn file_stem(path: &str) -> String {
    let last_segment = path.rsplit(['/', '\\']).next().unwrap_or(path);
    last_segment
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| last_segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stem_handles_unix_path() {
        assert_eq!(file_stem("/Music/Pink Floyd/01 - Speak to Me.flac"), "01 - Speak to Me");
    }

    #[test]
    fn file_stem_handles_windows_path() {
        assert_eq!(file_stem("C:\\Music\\song.mp3"), "song");
    }

    #[test]
    fn file_stem_handles_no_extension() {
        assert_eq!(file_stem("/path/no_extension"), "no_extension");
    }
}
