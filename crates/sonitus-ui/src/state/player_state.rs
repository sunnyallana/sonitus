//! Player state — current track, position, queue snapshot, volume.
//!
//! The state is updated by an event-pump task that consumes
//! [`PlayerEvent`](sonitus_core::player::PlayerEvent) from the engine and
//! writes them into a `Signal<PlayerState>`. UI components subscribe via
//! `use_context::<Signal<PlayerState>>()`.

use dioxus::prelude::*;
use sonitus_core::library::Track;
use sonitus_core::player::commands::RepeatMode;

/// User-visible playback state.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    /// Currently-playing track, if any.
    pub track: Option<Track>,
    /// Position in milliseconds.
    pub position_ms: u64,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Linear volume `0.0..=1.0`.
    pub volume: f32,
    /// Whether playback is paused.
    pub is_paused: bool,
    /// Snapshot of the queue.
    pub queue: Vec<Track>,
    /// Repeat mode.
    pub repeat: RepeatMode,
    /// Whether shuffle is on.
    pub shuffle: bool,
    /// Output device name.
    pub output_device: String,
    /// Last error from the engine, if any (transient — cleared on next event).
    pub last_error: Option<String>,
}

/// Install a `Signal<PlayerState>` into Dioxus context.
pub fn install_player_state() {
    use_context_provider(|| Signal::new(PlayerState {
        volume: 1.0,
        repeat: RepeatMode::Off,
        ..Default::default()
    }));
}
