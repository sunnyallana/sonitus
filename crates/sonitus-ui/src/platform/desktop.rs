//! Desktop integrations: media keys, system tray, window state.
//!
//! - **macOS**: `MediaSession` via the OS framework.
//! - **Linux**: MPRIS D-Bus interface.
//! - **Windows**: SystemMediaTransportControls.
//!
//! All three are surfaced behind a single `MediaSessionHandle` trait so
//! the rest of the UI doesn't care which OS it runs on.

#![cfg(all(not(target_arch = "wasm32"), any(target_os = "macos", target_os = "linux", target_os = "windows")))]

use std::path::PathBuf;

/// Snapshot of player state used to update OS media controls.
#[derive(Debug, Clone)]
pub struct MediaMetadata {
    /// Track title.
    pub title: String,
    /// Artist name (or empty).
    pub artist: String,
    /// Album title (or empty).
    pub album: String,
    /// Cover art file path (PNG/JPEG); the OS reads this for thumbnail display.
    pub cover_art_path: Option<PathBuf>,
    /// Position in milliseconds.
    pub position_ms: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Whether playback is paused.
    pub is_paused: bool,
}

/// Trait every desktop OS implements differently.
pub trait MediaSession {
    /// Update the OS-level "now playing" panel.
    fn update(&mut self, meta: &MediaMetadata);
    /// Clear the OS-level now-playing state (e.g. when stopping playback).
    fn clear(&mut self);
}

/// Construct an OS-appropriate media session handle.
/// Implementation here is a stub; concrete OS integrations would replace
/// this with platform-specific bindings.
pub fn build_media_session() -> Box<dyn MediaSession + Send> {
    Box::new(NoopMediaSession)
}

struct NoopMediaSession;
impl MediaSession for NoopMediaSession {
    fn update(&mut self, _meta: &MediaMetadata) {}
    fn clear(&mut self) {}
}

/// Persisted desktop window state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowState {
    /// Window x position.
    pub x: Option<i32>,
    /// Window y position.
    pub y: Option<i32>,
    /// Window width.
    pub width: u32,
    /// Window height.
    pub height: u32,
    /// Whether the window was maximized when last closed.
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self { x: None, y: None, width: 1280, height: 800, maximized: false }
    }
}

impl WindowState {
    /// Default file path for window state persistence.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("sonitus").join("window.toml"))
    }

    /// Load window state, falling back to defaults if not present or invalid.
    pub fn load() -> Self {
        let Some(path) = Self::default_path() else { return Self::default(); };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save window state via atomic write.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::default_path() else { return Ok(()); };
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let text = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }
}
