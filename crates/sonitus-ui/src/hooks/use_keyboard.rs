//! Global keyboard shortcuts.
//!
//! - `Space` — play/pause
//! - `Cmd/Ctrl+K` — focus search bar
//! - `Cmd/Ctrl+,` — open settings
//! - `←`/`→` — seek 5s
//! - `↑`/`↓` — volume up/down
//! - `J`/`L` — seek 10s back/forward
//! - `M` — mute/unmute
//! - `S` — toggle shuffle
//! - `R` — cycle repeat mode

use dioxus::prelude::*;

/// Install global keyboard shortcuts. Call once near the app root.
pub fn install_global_shortcuts() {
    // The actual key dispatch is wired in the App component via
    // `onkeydown` on the body element. This hook function is a hook
    // for future extensibility (e.g. user-customizable shortcuts).
    let _ = use_signal(|| ());
}

/// Map of human-readable shortcut → description, used by the about page.
pub fn shortcut_list() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Space", "Play / Pause"),
        ("Cmd/Ctrl + K", "Focus search"),
        ("Cmd/Ctrl + ,", "Open settings"),
        ("← / →", "Seek 5 seconds"),
        ("J / L", "Seek 10 seconds"),
        ("↑ / ↓", "Volume up / down"),
        ("M", "Mute / unmute"),
        ("S", "Toggle shuffle"),
        ("R", "Cycle repeat"),
        ("N", "Next track"),
        ("P", "Previous track"),
    ]
}
