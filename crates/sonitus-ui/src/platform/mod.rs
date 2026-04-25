//! Platform-specific integrations.
//!
//! Each submodule is gated by `#[cfg]` so only the relevant code links
//! into the binary for a given platform.

#[cfg(all(not(target_arch = "wasm32"), any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub mod desktop;

#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(any(target_os = "ios", target_os = "android"))]
pub mod mobile;
