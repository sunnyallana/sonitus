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
    settings_state::{SettingsState, install_settings_state},
};
use dioxus::prelude::*;
use sonitus_core::config::Theme;
use sonitus_core::player::commands::PlayerCommand;

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
    crate::components::playlists::add_to_playlist_dialog::install_add_to_playlist_state();
    // NewPlaylistState lives at app scope so the add-to-playlist dialog
    // can chain into "+ New playlist" from outside the playlists page.
    use_context_provider(|| {
        Signal::new(
            crate::components::playlists::new_playlist_dialog::NewPlaylistState::default(),
        )
    });

    let player_signal = use_context::<Signal<PlayerState>>();
    let downloads_signal = use_context::<Signal<Vec<DownloadItem>>>();
    let library_signal = use_context::<Signal<LibraryState>>();
    let mut settings_signal = use_context::<Signal<SettingsState>>();

    // Provide the boot status as context so any component can grab the
    // handle when it's available (e.g. clicking play needs it).
    let mut boot_status = use_context_provider(|| Signal::new(BootStatus::Idle));

    // Trigger boot exactly once.
    use_future(move || async move {
        if !matches!(*boot_status.read(), BootStatus::Idle) { return; }
        boot_status.set(BootStatus::Loading);
        match orchestrator::boot(BootConfig { passphrase: "sonitus".into() }).await {
            Ok((handle, channels)) => {
                // Hydrate the settings signal from the AppConfig loaded
                // by the orchestrator, then apply the persisted volume
                // to the player engine.
                {
                    let mut s = settings_signal.write();
                    s.config = handle.config.clone();
                    s.volume = handle.config.last_volume;
                }
                let _ = handle.player.send(PlayerCommand::SetVolume {
                    amplitude: handle.config.last_volume,
                });

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
    let theme_attr = match settings_signal.read().config.theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
        Theme::System => "system",
    };

    // The font preload Link is intentionally omitted: Dioxus 0.7's
    // document::Link doesn't expose the `as` attribute (Rust keyword); we'll
    // bring back an explicit raw-HTML preload in a follow-up.
    let _ = FONT_INTER;

    rsx! {
        document::Stylesheet { href: STYLE_CSS }
        document::Title { "Sonitus" }
        document::Meta { name: "color-scheme", content: "dark light" }

        // Wrap the app in a themed root so CSS custom properties under
        // [data-theme="..."] resolve. We re-read settings_signal here so
        // a live theme change re-renders this wrapper.
        div { "data-theme": "{theme_attr}", class: "themed-root",
            match status {
                BootStatus::Idle | BootStatus::Loading => rsx! { BootScreen {} },
                BootStatus::Ready(_) => rsx! { Router::<Route> {} },
                BootStatus::Failed(msg) => rsx! { BootError { message: msg } },
            }
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
