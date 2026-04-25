//! Schema structs for the `.sonitus` library file.
//!
//! All structs derive `Serialize`/`Deserialize` for `toml` round-tripping.
//! The format is designed to be human-readable and human-editable —
//! Sonitus reads what users save, but users can also hand-edit safely.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Top-level file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryMeta {
    /// File-level metadata.
    pub meta: FileMeta,

    /// Privacy preferences.
    #[serde(default)]
    pub privacy: PrivacyConfig,

    /// Audio engine preferences.
    #[serde(default)]
    pub audio: AudioConfig,

    /// UI appearance preferences.
    #[serde(default)]
    pub appearance: AppearanceConfig,

    /// Storage / cache preferences.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Configured sources.
    #[serde(default, rename = "sources")]
    pub sources: Vec<SourceDef>,

    /// User-defined playlists.
    #[serde(default, rename = "playlists")]
    pub playlists: Vec<PlaylistDef>,
}

/// File-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    /// Format version string (e.g. `"1.0"`).
    pub version: String,
    /// Numeric schema version for migrations.
    pub schema_version: u32,
    /// Always `"sonitus"`.
    #[serde(default = "default_app_name")]
    pub app: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
    /// Optional human-readable label for the device that created the file.
    #[serde(default)]
    pub device_name: Option<String>,
}

fn default_app_name() -> String { "sonitus".to_string() }

/// Privacy preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// MusicBrainz / AcoustID lookups enabled.
    #[serde(default)]
    pub metadata_lookups_enabled: bool,
    /// Always false; field exists so auditors can verify.
    #[serde(default)]
    pub telemetry_enabled: bool,
    /// Whether the audit log is being written.
    #[serde(default = "default_true")]
    pub audit_log_enabled: bool,
    /// Audit log size cap in megabytes.
    #[serde(default = "default_audit_size")]
    pub audit_log_max_size_mb: u64,
    /// Number of rotated logs to keep.
    #[serde(default = "default_audit_rotations")]
    pub audit_log_keep_rotations: u32,
    /// Always false.
    #[serde(default)]
    pub crash_reporting_enabled: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            metadata_lookups_enabled: false,
            telemetry_enabled: false,
            audit_log_enabled: true,
            audit_log_max_size_mb: 5,
            audit_log_keep_rotations: 3,
            crash_reporting_enabled: false,
        }
    }
}

fn default_true() -> bool { true }
fn default_audit_size() -> u64 { 5 }
fn default_audit_rotations() -> u32 { 3 }

/// Audio engine preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// `off`, `track`, `album`.
    #[serde(default = "default_replay_mode")]
    pub replay_gain_mode: String,
    /// Crossfade in seconds. 0.0 = off.
    #[serde(default)]
    pub crossfade_seconds: f32,
    /// Whether gapless playback is enabled.
    #[serde(default = "default_true")]
    pub gapless_enabled: bool,
    /// `small`, `medium`, `large`.
    #[serde(default = "default_buffer")]
    pub buffer_size: String,
    /// Preferred output device name (empty = system default).
    #[serde(default)]
    pub preferred_output_device: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            replay_gain_mode: "track".into(),
            crossfade_seconds: 0.0,
            gapless_enabled: true,
            buffer_size: "medium".into(),
            preferred_output_device: String::new(),
        }
    }
}

fn default_replay_mode() -> String { "track".into() }
fn default_buffer() -> String { "medium".into() }

/// UI appearance preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// `dark`, `light`, `system`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Hex color, e.g. `#1DB954`.
    #[serde(default = "default_accent")]
    pub accent_color: String,
    /// `small`, `medium`, `large`.
    #[serde(default = "default_font_size")]
    pub font_size: String,
    /// `grid` or `list`.
    #[serde(default = "default_view")]
    pub library_default_view: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            accent_color: "#1DB954".into(),
            font_size: "medium".into(),
            library_default_view: "grid".into(),
        }
    }
}

fn default_theme() -> String { "dark".into() }
fn default_accent() -> String { "#1DB954".into() }
fn default_font_size() -> String { "medium".into() }
fn default_view() -> String { "grid".into() }

/// Storage / cache preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Cache size cap in GB.
    #[serde(default = "default_cache_gb")]
    pub cache_max_gb: u64,
    /// Override default downloads location (empty = platform default).
    #[serde(default)]
    pub download_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            cache_max_gb: 10,
            download_path: String::new(),
        }
    }
}

fn default_cache_gb() -> u64 { 10 }

/// One source definition. The `kind` field discriminates which other
/// fields are meaningful — extra fields are kept around verbatim via TOML's
/// flatten support so we don't lose user-added context on round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    /// User-assigned ID, e.g. `"src_001"`.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// `local`, `google_drive`, `s3`, `smb`, `http`, `dropbox`, `onedrive`.
    pub kind: String,
    /// Whether scanning + playback is currently allowed.
    #[serde(default = "default_true")]
    pub enabled: bool,

    // Kind-specific fields. We list each as Optional so the same struct
    // serves all kinds; the `kind` field tells us which to expect.

    /// `local`: filesystem path.
    #[serde(default)]
    pub path: Option<String>,
    /// `google_drive`: folder ID to scope to.
    #[serde(default)]
    pub root_folder: Option<String>,
    /// `s3`: bucket name.
    #[serde(default)]
    pub bucket: Option<String>,
    /// `s3`: region.
    #[serde(default)]
    pub region: Option<String>,
    /// `s3`: optional endpoint URL for non-AWS providers.
    #[serde(default)]
    pub endpoint_url: Option<String>,
    /// `smb`/`http`: hostname or URL.
    #[serde(default)]
    pub host: Option<String>,
    /// `smb`: share name.
    #[serde(default)]
    pub share: Option<String>,
    /// `smb`: subdirectory under the share.
    #[serde(default)]
    pub base_path: Option<String>,
    /// `http`: base URL.
    #[serde(default)]
    pub url: Option<String>,
    /// `onedrive`: tenant (`common` or org GUID).
    #[serde(default)]
    pub tenant: Option<String>,
}

/// One playlist definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDef {
    /// Stable playlist ID.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last edit timestamp.
    pub updated_at: DateTime<Utc>,

    /// True for smart playlists.
    #[serde(default)]
    pub is_smart: bool,
    /// Smart-playlist rules (free-form TOML; the engine parses).
    #[serde(default)]
    pub smart_rules: Option<toml::Value>,

    /// Manual playlist track membership. Each ref pins a (source_id, path)
    /// pair so playlists stay valid even when track UUIDs change between
    /// rescans (different content hash → different ID).
    #[serde(default)]
    pub track_refs: Vec<TrackRef>,
}

/// Reference from a playlist to a track. Source-relative, format-stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRef {
    /// Source ID matching `SourceDef::id`.
    pub source_id: String,
    /// Path within the source.
    pub path: String,
}

impl Default for LibraryMeta {
    /// A fresh, empty library. The `meta.created_at` / `updated_at` fields
    /// are filled in with `Utc::now()`.
    fn default() -> Self {
        let now = Utc::now();
        Self {
            meta: FileMeta {
                version: "1.0".into(),
                schema_version: super::CURRENT_SCHEMA_VERSION,
                app: "sonitus".into(),
                created_at: now,
                updated_at: now,
                device_name: None,
            },
            privacy: PrivacyConfig::default(),
            audio: AudioConfig::default(),
            appearance: AppearanceConfig::default(),
            storage: StorageConfig::default(),
            sources: Vec::new(),
            playlists: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_library_meta_has_no_sources_or_playlists() {
        let m = LibraryMeta::default();
        assert!(m.sources.is_empty());
        assert!(m.playlists.is_empty());
        assert_eq!(m.meta.app, "sonitus");
    }

    #[test]
    fn default_privacy_disables_lookups_and_telemetry() {
        let p = PrivacyConfig::default();
        assert!(!p.metadata_lookups_enabled);
        assert!(!p.telemetry_enabled);
        assert!(!p.crash_reporting_enabled);
        assert!(p.audit_log_enabled);
    }
}
