//! Settings state — user preferences mirrored into UI signals.
//!
//! Mirror of [`AppConfig`](sonitus_core::AppConfig) plus consent toggles.
//! On startup, loaded from disk; on change, persisted via the orchestrator.

use dioxus::prelude::*;
use sonitus_core::config::{AppConfig, BufferSize, ReplayGainMode, Theme};

/// User-facing settings state.
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// The mirrored `AppConfig`.
    pub config: AppConfig,
    /// MusicBrainz consent.
    pub consent_metadata_lookups: bool,
    /// AcoustID consent.
    pub consent_acoustid: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            config: AppConfig::default(),
            consent_metadata_lookups: false,
            consent_acoustid: false,
        }
    }
}

impl SettingsState {
    /// Update theme.
    pub fn set_theme(&mut self, theme: Theme) { self.config.theme = theme; }
    /// Update ReplayGain mode.
    pub fn set_replay_gain(&mut self, mode: ReplayGainMode) { self.config.replay_gain_mode = mode; }
    /// Update buffer size.
    pub fn set_buffer_size(&mut self, size: BufferSize) { self.config.buffer_size = size; }
}

/// Install a `Signal<SettingsState>` into the context.
pub fn install_settings_state() {
    use_context_provider(|| Signal::new(SettingsState::default()));
}
