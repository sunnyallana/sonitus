//! Root component — installs all global context and the router.
//!
//! Boot lifecycle:
//!
//! 1. Mount Signals for player/library/download/search/settings state.
//! 2. Provide a `Signal<Option<AppHandle>>` via context (initially `None`).
//! 3. Run [`orchestrator::boot`] in a `use_future`. On success, write the
//!    handle into the signal and spawn the event pump.
//! 4. Render either a splash screen or the routed UI based on the signal.

use crate::orchestrator::{self, AppHandle, BootConfig};
use crate::routes::Route;
use crate::state::{
    download_state::{DownloadItem, install_download_state},
    library_state::{LibraryState, install_library_state},
    player_state::{PlayerState, install_player_state},
    search_state::install_search_state,
    settings_state::install_settings_state,
};
use dioxus::prelude::*;

const STYLE_CSS: Asset = asset!("/assets/styles/app.css");
const FONT_INTER: Asset = asset!("/assets/fonts/Inter-Variable.woff2");

/// Boot status surfaced to the UI. Stored in a `Signal` so components can
/// react to transitions (locked → ready) without redundant boot triggers.
#[derive(Clone, Default)]
pub enum BootStatus {
    /// Boot has not yet started (initial mount).
    #[default]
    Idle,
    /// Boot is in flight.
    Loading,
    /// Boot succeeded; the handle is available.
    Ready(AppHandle),
    /// Boot failed; error string for the UI to render.
    Failed(String),
}

/// Top-level app component. Mounts `Router` plus global Signals.
#[component]
pub fn App() -> Element {
    install_settings_state();
    install_library_state();
    install_player_state();
    install_download_state();
    install_search_state();

    let player_signal = use_context::<Signal<PlayerState>>();
    let downloads_signal = use_context::<Signal<Vec<DownloadItem>>>();
    let library_signal = use_context::<Signal<LibraryState>>();

    // Provide the boot status as context so any component can grab the
    // handle when it's available (e.g. clicking play needs it).
    let mut boot_status = use_context_provider(|| Signal::new(BootStatus::Idle));

    // Trigger boot exactly once.
    use_future(move || async move {
        if !matches!(*boot_status.read(), BootStatus::Idle) { return; }
        boot_status.set(BootStatus::Loading);
        match orchestrator::boot(BootConfig { passphrase: "sonitus".into() }).await {
            Ok((handle, channels)) => {
                orchestrator::start_event_pump(
                    handle.clone(),
                    channels,
                    player_signal,
                    downloads_signal,
                    library_signal,
                );
                boot_status.set(BootStatus::Ready(handle));
            }
            Err(e) => {
                tracing::error!(error = %e, "boot failed");
                boot_status.set(BootStatus::Failed(e.to_string()));
            }
        }
    });

    let status = boot_status.read().clone();

    rsx! {
        document::Stylesheet { href: STYLE_CSS }
        document::Link { rel: "preload", as_: "font", href: FONT_INTER, crossorigin: "anonymous" }
        document::Title { "Sonitus" }
        document::Meta { name: "color-scheme", content: "dark light" }

        match status {
            BootStatus::Idle | BootStatus::Loading => rsx! { BootScreen {} },
            BootStatus::Ready(_) => rsx! { Router::<Route> {} },
            BootStatus::Failed(msg) => rsx! { BootError { message: msg } },
        }
    }
}

/// Convenience hook for any component that needs the live `AppHandle`.
/// Returns `None` while booting; components should render a fallback.
pub fn use_app_handle() -> Option<AppHandle> {
    let status = use_context::<Signal<BootStatus>>();
    match status.read().clone() {
        BootStatus::Ready(h) => Some(h),
        _ => None,
    }
}

#[component]
fn BootScreen() -> Element {
    rsx! {
        div { class: "boot-screen",
            div { class: "boot-screen__logo", "Sonitus" }
            div { class: "boot-screen__spinner", "Unlocking your library…" }
        }
    }
}

#[component]
fn BootError(message: String) -> Element {
    rsx! {
        div { class: "boot-screen",
            div { class: "boot-screen__logo", "Sonitus" }
            div { class: "boot-screen__error",
                h2 { "Couldn't open the library" }
                pre { class: "boot-screen__detail", "{message}" }
            }
        }
    }
}
