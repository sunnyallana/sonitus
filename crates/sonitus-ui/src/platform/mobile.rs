//! Mobile (iOS/Android) integrations: background audio, lock-screen controls.
//!
//! - **iOS**: `AVAudioSession` for background playback eligibility,
//!   `MPRemoteCommandCenter` + `MPNowPlayingInfoCenter` for lock-screen.
//! - **Android**: `AudioFocus` for ducking under notifications,
//!   `MediaSession` + `MediaSessionCompat` for lock-screen.

#![cfg(any(target_os = "ios", target_os = "android"))]

/// Configure the audio session for music playback (call once at startup).
pub fn configure_audio_session() {
    // FFI calls into platform frameworks would go here. We expose the
    // public surface so the orchestrator can call it; the actual JNI/objc
    // glue lives in dedicated bridge modules wired by the build system.
}

/// Acquire audio focus / activate the audio session before playback.
pub fn acquire_audio_focus() {}

/// Release audio focus after playback.
pub fn release_audio_focus() {}

/// Snapshot of metadata for the lock-screen controls.
#[derive(Debug, Clone)]
pub struct LockScreenMetadata {
    /// Track title.
    pub title: String,
    /// Artist name.
    pub artist: String,
    /// Album.
    pub album: String,
    /// Current position, ms.
    pub position_ms: u64,
    /// Duration, ms.
    pub duration_ms: u64,
    /// Whether playback is paused.
    pub is_paused: bool,
    /// Cover-art file URI (file://) for the OS to read.
    pub cover_art_uri: Option<String>,
}

/// Push metadata to the OS lock-screen controls.
pub fn update_lockscreen(_meta: &LockScreenMetadata) {}

/// Clear lock-screen controls (e.g. on stop).
pub fn clear_lockscreen() {}
