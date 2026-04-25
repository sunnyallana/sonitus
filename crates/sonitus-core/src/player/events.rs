//! Events emitted **out** of the player engine to the rest of the app.
//!
//! The decode thread sends these on a `crossbeam_channel::Sender<PlayerEvent>`
//! that the UI subscribes to. Events should be consumed promptly — channel
//! capacity is bounded.

use crate::library::Track;
use serde::{Deserialize, Serialize};

/// Lifecycle and progress events from the player engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerEvent {
    /// A track has begun playing.
    Playing {
        /// The track now playing.
        track: Track,
        /// Total duration in milliseconds (may differ slightly from the
        /// DB value if the file was re-tagged after scan).
        duration_ms: u64,
    },
    /// Playback has been paused.
    Paused {
        /// Position in milliseconds where playback was paused.
        position_ms: u64,
    },
    /// Playback has resumed from pause.
    Resumed {
        /// Position in milliseconds where playback resumed.
        position_ms: u64,
    },
    /// Playback has stopped (queue empty or explicit Stop).
    Stopped,
    /// Periodic progress update — emitted ~10x/second during playback.
    Progress {
        /// Current playback position in milliseconds.
        position_ms: u64,
        /// Total track duration in milliseconds.
        duration_ms: u64,
        /// Buffered position (gapless pre-decode of the *current* track).
        buffered_ms: u64,
    },
    /// The current track has finished. Comes before the next `Playing`.
    TrackEnded {
        /// The track that finished.
        track_id: String,
    },
    /// The queue or its order has changed.
    QueueChanged {
        /// New queue snapshot, in playback order.
        queue: Vec<Track>,
    },
    /// Volume has been changed (whether by user or remote control).
    VolumeChanged {
        /// New volume in `0.0..=1.0`.
        amplitude: f32,
    },
    /// Output audio device has changed.
    OutputDeviceChanged {
        /// Display name of the device (or `"<default>"`).
        device_name: String,
    },
    /// An error occurred. Playback may have stopped.
    Error {
        /// Human-readable message suitable for surfacing in the UI.
        message: String,
    },
}
