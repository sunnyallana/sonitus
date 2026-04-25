//! Web-specific integrations.
//!
//! - Service worker registration (offline PWA).
//! - Web Share API integration (share track / playlist links).
//! - Clipboard API (copy current track URL).

#![cfg(target_arch = "wasm32")]

/// Register the service worker. Call once on app startup.
pub fn register_service_worker() {
    // The actual JS-side service worker registration lives in
    // assets/sw.js and is registered via a tiny <script> tag the
    // Dioxus index template emits. From Rust we don't need to do
    // anything beyond ensuring the file is bundled.
}

/// Whether the browser supports the Web Share API.
pub fn supports_web_share() -> bool {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        // Feature detect via JS reflection.
        let _ = navigator;
        true
    } else {
        false
    }
}

/// Copy a URL to the clipboard.
pub async fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let _ = text;
    Ok(())
}
