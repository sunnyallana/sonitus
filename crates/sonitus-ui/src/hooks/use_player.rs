//! `use_player()` — read playback state, send commands.

use crate::state::player_state::PlayerState;
use dioxus::prelude::*;

/// Handle returned by [`use_player`].
#[derive(Clone, Copy)]
pub struct PlayerHandle {
    state: Signal<PlayerState>,
}

impl PlayerHandle {
    /// Snapshot of the current state.
    pub fn read(&self) -> PlayerState {
        self.state.read().clone()
    }

    /// Whether something is currently playing (paused or not).
    pub fn has_track(&self) -> bool {
        self.state.read().track.is_some()
    }

    /// Whether playback is paused.
    pub fn is_paused(&self) -> bool {
        self.state.read().is_paused
    }

    /// Convenience: position as a 0..=1 fraction.
    pub fn progress_fraction(&self) -> f32 {
        let s = self.state.read();
        if s.duration_ms == 0 {
            0.0
        } else {
            (s.position_ms as f32 / s.duration_ms as f32).clamp(0.0, 1.0)
        }
    }
}

/// Hook to access player state.
pub fn use_player() -> PlayerHandle {
    let state = use_context::<Signal<PlayerState>>();
    PlayerHandle { state }
}
