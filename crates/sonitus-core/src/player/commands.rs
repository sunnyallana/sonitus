//! Commands sent **into** the player engine from the UI / orchestrator.
//!
//! All commands are non-blocking: the decode thread receives them on a
//! `crossbeam_channel::Receiver<PlayerCommand>` and processes them on the
//! next decode cycle (typically within 50ms).

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// What the player engine should do next.
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    /// Load and start playing the given track from the library.
    /// The decode thread will fetch its source via the source registry.
    Play {
        /// The library track ID.
        track_id: String,
    },
    /// Play directly from a URL (used for HTTP source streaming and
    /// previews where no DB row exists yet).
    PlayUrl {
        /// The URL to stream.
        url: String,
    },
    /// Pause playback. The buffer stays primed; resume is instant.
    Pause,
    /// Resume from a paused state.
    Resume,
    /// Stop playback. Clears the buffer; resume requires re-decoding.
    Stop,
    /// Seek to `seconds` from the start of the current track.
    Seek {
        /// Position in fractional seconds.
        seconds: f64,
    },
    /// Set output volume in `0.0..=1.0` (linear amplitude — UI applies
    /// any logarithmic curve before sending).
    SetVolume {
        /// Linear amplitude.
        amplitude: f32,
    },
    /// Skip to the next track in the queue.
    Next,
    /// Go to the previous track. Restarts the current track if more
    /// than 3 seconds have played.
    Prev,
    /// Append a track to the end of the queue.
    Enqueue {
        /// The library track ID.
        track_id: String,
    },
    /// Insert a track immediately after the currently-playing one.
    EnqueueNext {
        /// The library track ID.
        track_id: String,
    },
    /// Remove every track from the queue except the currently-playing one.
    ClearQueue,
    /// Set whether shuffle is enabled.
    SetShuffle {
        /// Whether shuffle is on.
        enabled: bool,
    },
    /// Set the repeat mode.
    SetRepeat {
        /// New repeat mode.
        mode: RepeatMode,
    },
    /// Set the output device by name (matches a string from
    /// `output_native::list_devices`). Pass `None` to use the system default.
    SetOutputDevice {
        /// Device name, or `None` for default.
        name: Option<String>,
    },
    /// Apply a new ReplayGain mode.
    SetReplayGain {
        /// New mode.
        mode: ReplayGainCommand,
    },
    /// Tell the engine to shut down — the decode thread joins after
    /// processing this command.
    Shutdown,
}

/// Repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RepeatMode {
    /// Stop after the queue is exhausted.
    Off,
    /// Repeat the current track indefinitely.
    One,
    /// Repeat the entire queue.
    All,
}

/// ReplayGain mode used for command messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ReplayGainCommand {
    /// No gain adjustment.
    Off,
    /// Use track-level ReplayGain.
    Track,
    /// Use album-level ReplayGain.
    Album,
}
