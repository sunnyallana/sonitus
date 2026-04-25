//! Domain models — `Track`, `Album`, `Artist`, `Playlist`, `Source`.
//!
//! Every struct here mirrors a row in the SQLite schema. They derive
//! `sqlx::FromRow` so query results decode automatically.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::path::PathBuf;
use strum::{Display, EnumString};

/// One row from `tracks`. Joins (artist name, album title) are not
/// embedded here — query modules return enriched view structs when needed.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Track {
    /// UUID v4. Stable across rescans (computed from source_id+remote_path).
    pub id: String,
    /// Track title.
    pub title: String,
    /// Artist FK (may be null for orphaned tracks).
    pub artist_id: Option<String>,
    /// Album-artist FK (used for "Various Artists" compilations).
    pub album_artist_id: Option<String>,
    /// Album FK.
    pub album_id: Option<String>,
    /// Source FK — which source this track lives on.
    pub source_id: String,
    /// Path within the source — e.g. `"/Music/Pink Floyd/DSOTM/01.flac"`.
    pub remote_path: String,
    /// Cached local copy, if downloaded.
    pub local_cache_path: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Track number within the disc.
    pub track_number: Option<i32>,
    /// Disc number (1 for single-disc albums).
    pub disc_number: i32,
    /// Genre tag.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Beats per minute (set if available; not auto-detected by Sonitus).
    pub bpm: Option<f64>,
    /// EBU R128 track-level gain in dB.
    pub replay_gain_track: Option<f64>,
    /// EBU R128 album-level gain in dB.
    pub replay_gain_album: Option<f64>,
    /// File size in bytes.
    pub file_size_bytes: Option<i64>,
    /// Container/codec format.
    pub format: Option<String>,
    /// Bitrate in kilobits per second.
    pub bitrate_kbps: Option<i32>,
    /// Sample rate in Hz.
    pub sample_rate_hz: Option<i32>,
    /// Bit depth (16, 24, 32).
    pub bit_depth: Option<i32>,
    /// Number of audio channels (1 = mono, 2 = stereo, 6 = 5.1, etc.).
    pub channels: Option<i32>,
    /// BLAKE3 hash of the file's bytes (used for cache keys, dedup).
    pub content_hash: Option<String>,
    /// MusicBrainz recording ID, if set via metadata lookup.
    pub musicbrainz_id: Option<String>,
    /// User play count.
    pub play_count: i64,
    /// Unix epoch of last play.
    pub last_played_at: Option<i64>,
    /// User-set rating, 0-5 stars (NULL = no rating).
    pub rating: Option<i32>,
    /// "Loved" flag — separate from rating, used for shuffled queues.
    pub loved: i32,
    /// Unix epoch of insertion into the library.
    pub created_at: i64,
    /// Unix epoch of last metadata update.
    pub updated_at: i64,
}

impl Track {
    /// Derive a new stable UUID v5-style ID from a source + remote path.
    /// We hash via BLAKE3 (256-bit), then format as a UUID for portability.
    pub fn derive_id(source_id: &str, remote_path: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(source_id.as_bytes());
        hasher.update(b":");
        hasher.update(remote_path.as_bytes());
        let h = hasher.finalize();
        // Use first 16 bytes as a UUID.
        let bytes = h.as_bytes();
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&bytes[..16]);
        // Set version (5) and variant per RFC 4122
        id_bytes[6] = (id_bytes[6] & 0x0F) | 0x50;
        id_bytes[8] = (id_bytes[8] & 0x3F) | 0x80;
        uuid::Uuid::from_bytes(id_bytes).to_string()
    }

    /// `loved` decoded as a bool.
    pub fn is_loved(&self) -> bool {
        self.loved != 0
    }
}

/// One row from `artists`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Artist {
    /// UUID derived from sort_name.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Sort key (e.g. "Beatles, The"). Drives alphabetical browsing.
    pub sort_name: String,
    /// MusicBrainz artist ID, if set.
    pub musicbrainz_id: Option<String>,
    /// Optional biography text.
    pub bio: Option<String>,
    /// External image URL (for artists from MusicBrainz).
    pub image_url: Option<String>,
    /// Embedded image bytes (rendered in offline mode).
    pub image_blob: Option<Vec<u8>>,
    /// Aggregated play count across all tracks.
    pub play_count: i64,
    /// Unix epoch of insertion.
    pub created_at: i64,
    /// Unix epoch of last update.
    pub updated_at: i64,
}

impl Artist {
    /// Derive a stable artist ID from the sort name. Different name
    /// spellings of the same artist will be merged.
    pub fn derive_id(sort_name: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"artist:");
        hasher.update(sort_name.to_lowercase().as_bytes());
        let h = hasher.finalize();
        let bytes = h.as_bytes();
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&bytes[..16]);
        id_bytes[6] = (id_bytes[6] & 0x0F) | 0x50;
        id_bytes[8] = (id_bytes[8] & 0x3F) | 0x80;
        uuid::Uuid::from_bytes(id_bytes).to_string()
    }

    /// Compute a sort name from a display name: drop leading "The ",
    /// preserve case otherwise.
    pub fn sort_name_for(name: &str) -> String {
        let trimmed = name.trim();
        if let Some(rest) = trimmed.strip_prefix("The ") {
            format!("{rest}, The")
        } else if let Some(rest) = trimmed.strip_prefix("the ") {
            format!("{rest}, the")
        } else {
            trimmed.to_string()
        }
    }
}

/// One row from `albums`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Album {
    /// UUID derived from artist + title.
    pub id: String,
    /// Album title.
    pub title: String,
    /// Primary artist FK.
    pub artist_id: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Embedded cover art bytes.
    pub cover_art_blob: Option<Vec<u8>>,
    /// External cover art URL (MusicBrainz).
    pub cover_art_url: Option<String>,
    /// BLAKE3 hash of cover art for de-duplication.
    pub cover_art_hash: Option<String>,
    /// MusicBrainz release ID.
    pub musicbrainz_id: Option<String>,
    /// Total tracks per the album metadata.
    pub total_tracks: Option<i32>,
    /// Number of discs in the release.
    pub disc_count: i32,
    /// Aggregated play count.
    pub play_count: i64,
    /// Unix epoch of insertion.
    pub created_at: i64,
    /// Unix epoch of last update.
    pub updated_at: i64,
}

impl Album {
    /// Derive a stable album ID from artist + title.
    pub fn derive_id(artist_id: Option<&str>, title: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"album:");
        hasher.update(artist_id.unwrap_or("").as_bytes());
        hasher.update(b":");
        hasher.update(title.to_lowercase().as_bytes());
        let h = hasher.finalize();
        let bytes = h.as_bytes();
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&bytes[..16]);
        id_bytes[6] = (id_bytes[6] & 0x0F) | 0x50;
        id_bytes[8] = (id_bytes[8] & 0x3F) | 0x80;
        uuid::Uuid::from_bytes(id_bytes).to_string()
    }
}

/// One row from `playlists`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Playlist {
    /// UUID v4.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Embedded cover art (collage of contained albums).
    pub cover_art: Option<Vec<u8>>,
    /// Smart-playlist flag (0 = manual, 1 = rule-driven).
    pub is_smart: i32,
    /// JSON-encoded rules if `is_smart` is set.
    pub smart_rules: Option<String>,
    /// Cached count of contained tracks.
    pub track_count: i64,
    /// Cached total duration in ms.
    pub total_duration_ms: i64,
    /// Unix epoch of creation.
    pub created_at: i64,
    /// Unix epoch of last edit.
    pub updated_at: i64,
}

impl Playlist {
    /// Whether this playlist is rule-driven (smart) vs manual.
    pub fn is_smart(&self) -> bool {
        self.is_smart != 0
    }
}

/// One row from `sources`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Source {
    /// User-assigned ID (e.g. `"src_001"`).
    pub id: String,
    /// User-visible name (e.g. `"Home NAS"`).
    pub name: String,
    /// Source kind discriminator.
    pub kind: String,
    /// JSON-encoded non-secret config.
    pub config_json: String,
    /// Encrypted credential blob (XChaCha20-Poly1305).
    pub credentials_enc: Option<Vec<u8>>,
    /// Current scan state.
    pub scan_state: String,
    /// Unix epoch of last scan completion.
    pub last_scanned_at: Option<i64>,
    /// Last error message if `scan_state == "error"`.
    pub last_error: Option<String>,
    /// Cached track count.
    pub track_count: i64,
    /// Whether the user has the source enabled.
    pub enabled: i32,
    /// Unix epoch of creation.
    pub created_at: i64,
    /// Unix epoch of last update.
    pub updated_at: i64,
}

impl Source {
    /// Whether this source is currently enabled for scanning/streaming.
    pub fn is_enabled(&self) -> bool {
        self.enabled != 0
    }
}

/// Source kinds — string-encoded in the DB, typed at the Rust layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Local filesystem.
    Local,
    /// Google Drive (OAuth2 PKCE).
    GoogleDrive,
    /// AWS S3 (or any S3-compatible service).
    S3,
    /// SMB / CIFS.
    Smb,
    /// Generic HTTP directory listing + byte-range.
    Http,
    /// Dropbox.
    Dropbox,
    /// Microsoft OneDrive.
    Onedrive,
}

/// Lifecycle of a source scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ScanState {
    /// Not currently scanning.
    Idle,
    /// Scan in progress.
    Scanning,
    /// Last scan failed; see `last_error`.
    Error,
}

/// Audio container/codec format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TrackFormat {
    /// MPEG-1/2 Layer III.
    Mp3,
    /// Free Lossless Audio Codec.
    Flac,
    /// Vorbis-in-Ogg.
    Ogg,
    /// Advanced Audio Coding (typically in M4A/MP4).
    Aac,
    /// Linear PCM.
    Wav,
    /// Opus.
    Opus,
    /// Apple Lossless.
    Alac,
    /// Audio Interchange File Format.
    Aiff,
}

impl TrackFormat {
    /// Try to recognize a format from a filename extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mp3"        => Some(Self::Mp3),
            "flac"       => Some(Self::Flac),
            "ogg" | "oga"=> Some(Self::Ogg),
            "aac" | "m4a"=> Some(Self::Aac),
            "mp4"        => Some(Self::Aac), // most music in MP4 is AAC
            "wav"        => Some(Self::Wav),
            "opus"       => Some(Self::Opus),
            "alac"       => Some(Self::Alac),
            "aiff" | "aif"=> Some(Self::Aiff),
            _ => None,
        }
    }
}

/// Scan progress event sent over a channel from scanner → UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressEvent {
    /// Source being scanned.
    pub source_id: String,
    /// Files discovered so far.
    pub files_found: u64,
    /// Tracks indexed so far.
    pub tracks_indexed: u64,
    /// Files that failed to parse.
    pub files_failed: u64,
    /// Currently-processing file path.
    pub current_path: Option<String>,
}

/// Helper: convert a `chrono::DateTime<Utc>` to a Unix timestamp suitable
/// for the SQLite columns we use as `INTEGER NOT NULL`.
pub fn to_epoch(dt: DateTime<Utc>) -> i64 {
    dt.timestamp()
}

/// Helper: build a `PathBuf` for a track's local cache, given its hash.
pub fn cache_path_for(cache_dir: &PathBuf, content_hash: &str) -> PathBuf {
    // 2-char prefix shards keep the cache directory from blowing up flat.
    let (shard, rest) = content_hash.split_at(2.min(content_hash.len()));
    cache_dir.join(shard).join(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_id_is_stable_across_calls() {
        let a = Track::derive_id("src_1", "/path/foo.mp3");
        let b = Track::derive_id("src_1", "/path/foo.mp3");
        assert_eq!(a, b);
    }

    #[test]
    fn track_id_differs_for_different_paths() {
        let a = Track::derive_id("src_1", "/a.mp3");
        let b = Track::derive_id("src_1", "/b.mp3");
        assert_ne!(a, b);
    }

    #[test]
    fn artist_sort_name_drops_leading_the() {
        assert_eq!(Artist::sort_name_for("The Beatles"), "Beatles, The");
        assert_eq!(Artist::sort_name_for("Pink Floyd"), "Pink Floyd");
        assert_eq!(Artist::sort_name_for("the doors"), "doors, the");
    }

    #[test]
    fn artist_id_is_case_insensitive() {
        let a = Artist::derive_id("Beatles, The");
        let b = Artist::derive_id("BEATLES, THE");
        assert_eq!(a, b, "casing differences should not split the artist");
    }

    #[test]
    fn track_format_recognizes_common_extensions() {
        assert_eq!(TrackFormat::from_extension("mp3"),  Some(TrackFormat::Mp3));
        assert_eq!(TrackFormat::from_extension("FLAC"), Some(TrackFormat::Flac));
        assert_eq!(TrackFormat::from_extension("m4a"),  Some(TrackFormat::Aac));
        assert_eq!(TrackFormat::from_extension("xyz"),  None);
    }

    #[test]
    fn cache_path_shards_first_two_chars() {
        let dir = PathBuf::from("/cache");
        let p = cache_path_for(&dir, "abcdef0123");
        assert_eq!(p, PathBuf::from("/cache/ab/cdef0123"));
    }
}
