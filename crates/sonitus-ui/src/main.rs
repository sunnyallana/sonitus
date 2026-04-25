//! Sonitus UI — entry point.
//!
//! Dispatches to the right Dioxus launcher per platform via `#[cfg]`.
//! On the web target, this is the WASM entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Dioxus components are PascalCase by convention; this allow keeps rust-analyzer
// from yelling about every component name.
#![allow(non_snake_case)]
// Many state structs and platform-shim helpers are intentionally part of
// the public surface for future wiring (e.g. preference fields the
// settings UI doesn't yet read, MediaSession trait for OS media keys).
// Allow dead-code at crate level rather than scattering #[allow] everywhere.
#![allow(dead_code)]

mod app;
mod components;
mod hooks;
mod orchestrator;
mod platform;
mod routes;
mod state;

use app::App;

fn main() {
    // Initialize logging via sonitus-core. On the web target this becomes
    // a no-op (tracing-subscriber's writer panics on wasm without a JS
    // bridge); we install the wasm console layer instead.
    sonitus_core::init_logging();

    // Dioxus 0.7 unified launcher — picks the right backend at runtime
    // based on enabled features.
    dioxus::launch(App);
}
