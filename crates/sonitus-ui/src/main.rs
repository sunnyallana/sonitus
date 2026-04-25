//! Sonitus UI — entry point.
//!
//! Dispatches to the right Dioxus launcher per platform via `#[cfg]`.
//! On the web target, this is the WASM entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Dioxus components are PascalCase by convention; this allow keeps rust-analyzer
// from yelling about every component name.
#![allow(non_snake_case)]

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
