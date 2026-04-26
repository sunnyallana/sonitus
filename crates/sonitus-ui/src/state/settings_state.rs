//! Settings state — user preferences mirrored into UI signals.
//!
//! Reflects [`AppConfig`](sonitus_core::AppConfig) loaded from disk at boot.
//! Mutators write through to disk via [`AppConfig::save`] so changes
//! persist across launches. The persisted file lives at
//! `%APPDATA%/sonitus/config.toml` on Windows.

use dioxus::prelude::*;
use sonitus_core::config::{AppConfig, BufferSize, LibraryView, ReplayGainMode, Theme};
use std::path::PathBuf;

/// User-facing settings state.
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// The mirrored `AppConfig`.
    pub config: AppConfig,
    /// MusicBrainz consent.
    pub consent_metadata_lookups: bool,
    /// AcoustID consent.
    pub consent_acoustid: bool,
    /// User-facing volume in 0.0..=1.0. Synced separately from config
    /// (config holds it on disk; this is the live value for the UI).
    pub volume: f32,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            config: AppConfig::default(),
            consent_metadata_lookups: false,
            consent_acoustid: false,
            volume: 1.0,
        }
    }
}

impl SettingsState {
    /// Update theme + persist.
    pub fn set_theme(&mut self, theme: Theme) {
        self.config.theme = theme;
        self.persist();
    }
    /// Update ReplayGain mode + persist.
    pub fn set_replay_gain(&mut self, mode: ReplayGainMode) {
        self.config.replay_gain_mode = mode;
        self.persist();
    }
    /// Update buffer size + persist.
    pub fn set_buffer_size(&mut self, size: BufferSize) {
        self.config.buffer_size = size;
        self.persist();
    }
    /// Update volume + persist. The player engine is the authoritative
    /// source for live playback volume; this records the value for the
    /// next launch.
    pub fn set_volume(&mut self, vol: f32) {
        let v = vol.clamp(0.0, 1.0);
        self.volume = v;
        // Volume isn't part of AppConfig today; store on disk via a
        // small ad-hoc rider next to it. Keeping this simple: AppConfig
        // gets a new field below.
        self.config.last_volume = v;
        self.persist();
    }

    /// Update crossfade duration + persist.
    pub fn set_crossfade_secs(&mut self, secs: f32) {
        self.config.crossfade_secs = secs.clamp(0.0, 12.0);
        self.persist();
    }
    /// Update gapless flag + persist.
    pub fn set_gapless(&mut self, on: bool) {
        self.config.gapless_enabled = on;
        self.persist();
    }
    /// Update output device override + persist. `None` = system default.
    pub fn set_output_device(&mut self, name: Option<String>) {
        self.config.output_device = name;
        self.persist();
    }
    /// Update cache size limit (MiB) + persist.
    pub fn set_cache_max_mb(&mut self, mb: u64) {
        self.config.cache_max_mb = mb.max(1);
        self.persist();
    }
    /// Update download location override + persist.
    pub fn set_download_location(&mut self, path: Option<PathBuf>) {
        self.config.download_location = path;
        self.persist();
    }
    /// Update library default view + persist.
    pub fn set_library_view(&mut self, view: LibraryView) {
        self.config.library_view = view;
        self.persist();
    }

    fn persist(&self) {
        if let Err(e) = self.config.save() {
            tracing::warn!(error = %e, "settings persist failed");
        }
    }
}

/// Install a `Signal<SettingsState>` into the context.
pub fn install_settings_state() {
    use_context_provider(|| Signal::new(SettingsState::default()));
}
