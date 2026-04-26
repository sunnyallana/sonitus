//! Application configuration — paths, defaults, and persistence.
//!
//! `AppConfig` holds **non-secret** runtime configuration. Secrets (OAuth
//! tokens, source passwords, vault keys) live elsewhere — see the [`crypto`]
//! module. The user-facing data file is the `.sonitus` library file
//! (handled by the `sonitus-meta` crate); this struct is the application's
//! own preferences.
//!
//! Paths follow platform conventions via the `dirs` crate:
//!
//! | Platform | Config dir                              | Data dir                              |
//! |----------|-----------------------------------------|---------------------------------------|
//! | Linux    | `~/.config/sonitus`                     | `~/.local/share/sonitus`              |
//! | macOS    | `~/Library/Application Support/sonitus` | `~/Library/Application Support/sonitus` |
//! | Windows  | `%APPDATA%\sonitus`                      | `%LOCALAPPDATA%\sonitus`              |
//!
//! [`crypto`]: crate::crypto

use crate::error::{Result, SonitusError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Application-wide configuration loaded from `config.toml` at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Schema version of the config file. Bumped on breaking changes.
    pub config_version: u32,

    /// Path to the user's `.sonitus` library file.
    /// If `None`, the user has not yet created or imported a library.
    pub library_path: Option<PathBuf>,

    /// Maximum size of the offline cache, in megabytes.
    pub cache_max_mb: u64,

    /// Maximum size of the audit log file before rotation, in megabytes.
    pub audit_log_max_mb: u64,

    /// How many rotated audit log files to keep before deleting the oldest.
    pub audit_log_keep_rotations: u32,

    /// Network request timeout in seconds. Applies to all outbound HTTP.
    pub http_timeout_secs: u64,

    /// Maximum number of concurrent downloads.
    pub max_concurrent_downloads: usize,

    /// Audio: Default ReplayGain mode (`off`, `track`, `album`).
    pub replay_gain_mode: ReplayGainMode,

    /// Audio: Crossfade duration in seconds. Zero means no crossfade.
    pub crossfade_secs: f32,

    /// Audio: Whether gapless playback is enabled.
    pub gapless_enabled: bool,

    /// Audio: Output buffer size hint.
    pub buffer_size: BufferSize,

    /// UI: Theme preference.
    pub theme: Theme,

    /// UI: Accent color as a hex string `#RRGGBB`.
    pub accent_color: String,

    /// Last volume the user set, in `0.0..=1.0`. Restored on next launch
    /// so playback resumes at the level they left it. Default 1.0.
    #[serde(default = "default_volume")]
    pub last_volume: f32,
}

fn default_volume() -> f32 { 1.0 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: 1,
            library_path: None,
            cache_max_mb: 10_240, // 10 GB
            audit_log_max_mb: 5,
            audit_log_keep_rotations: 3,
            http_timeout_secs: 30,
            max_concurrent_downloads: 4,
            replay_gain_mode: ReplayGainMode::Track,
            crossfade_secs: 0.0,
            gapless_enabled: true,
            buffer_size: BufferSize::Medium,
            theme: Theme::System,
            accent_color: "#1DB954".to_string(),
            last_volume: 1.0,
        }
    }
}

/// ReplayGain normalization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplayGainMode {
    /// No gain adjustment.
    Off,
    /// Use per-track gain values.
    Track,
    /// Use per-album gain values (preserves intra-album dynamics).
    Album,
}

/// Audio output buffer size preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BufferSize {
    /// Lowest latency, highest CPU; may underrun on slow systems.
    Small,
    /// Balanced default.
    Medium,
    /// Highest latency, lowest CPU; smoothest playback.
    Large,
}

impl BufferSize {
    /// Frames per buffer (at 48 kHz stereo, multiply by 2 channels for samples).
    pub fn frames(self) -> u32 {
        match self {
            Self::Small => 256,
            Self::Medium => 1024,
            Self::Large => 4096,
        }
    }
}

/// Color theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Dark mode.
    Dark,
    /// Light mode.
    Light,
    /// Follow OS preference.
    System,
}

impl AppConfig {
    /// Return the platform-appropriate config directory for Sonitus,
    /// creating it if it does not exist.
    pub fn config_dir() -> Result<PathBuf> {
        let base = dirs::config_dir().ok_or(SonitusError::NoConfigDir)?;
        let dir = base.join("sonitus");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Return the platform-appropriate data directory (DB, cache).
    pub fn data_dir() -> Result<PathBuf> {
        let base = dirs::data_dir().ok_or(SonitusError::NoConfigDir)?;
        let dir = base.join("sonitus");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Path to the encrypted SQLite database.
    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("library.db"))
    }

    /// Path to the vault salt file. Plaintext is fine — the salt is not
    /// secret; only the passphrase is.
    pub fn vault_salt_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("vault.salt"))
    }

    /// Path to the audit log JSONL file.
    pub fn audit_log_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("audit.log"))
    }

    /// Path to the offline media cache directory.
    pub fn cache_dir() -> Result<PathBuf> {
        let dir = Self::data_dir()?.join("cache");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Path to the `config.toml` file holding `AppConfig`.
    pub fn config_file_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Load `AppConfig` from disk, or return the default if no file exists.
    pub fn load() -> Result<Self> {
        let path = Self::config_file_path()?;
        Self::load_from(&path)
    }

    /// Load from a specific path. Used by tests.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Persist `AppConfig` to disk via atomic write.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_file_path()?;
        self.save_to(&path)
    }

    /// Save to a specific path via temp-file + fsync + rename for crash safety.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        let parent = path.parent().ok_or_else(|| SonitusError::PathNotFound(path.to_path_buf()))?;
        std::fs::create_dir_all(parent)?;

        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        std::io::Write::write_all(&mut tmp, text.as_bytes())?;
        std::io::Write::flush(&mut tmp)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| SonitusError::Io(e.error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_config_round_trips_through_toml() {
        let cfg = AppConfig::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.config_version, cfg.config_version);
        assert_eq!(back.cache_max_mb, cfg.cache_max_mb);
        assert_eq!(back.replay_gain_mode, cfg.replay_gain_mode);
    }

    #[test]
    fn save_then_load_preserves_state() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = AppConfig::default();
        cfg.cache_max_mb = 42;
        cfg.theme = Theme::Light;
        cfg.save_to(&path).unwrap();
        let back = AppConfig::load_from(&path).unwrap();
        assert_eq!(back.cache_max_mb, 42);
        assert_eq!(back.theme, Theme::Light);
    }

    #[test]
    fn load_from_missing_path_returns_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does_not_exist.toml");
        let cfg = AppConfig::load_from(&path).unwrap();
        assert_eq!(cfg.config_version, AppConfig::default().config_version);
    }
}
