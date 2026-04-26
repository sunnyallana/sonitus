//! Dialog for adding a track to an existing manual playlist.
//!
//! Provided as a `Signal<AddToPlaylistState>` at the App-level scope so
//! any component can open it by setting `open: true, track_id: Some(...)`.
//! Lists every manual playlist; click one → appends the track. Smart
//! playlists are excluded since their membership is rule-driven.

use crate::app::use_app_handle;
use crate::components::playlists::new_playlist_dialog::NewPlaylistState;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// State for the add-to-playlist dialog.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AddToPlaylistState {
    /// Whether the dialog is open.
    pub open: bool,
    /// The track ID being added; required when open is true.
    pub track_id: Option<String>,
    /// Optional human-readable label shown in the dialog header.
    pub track_title: Option<String>,
    /// Filled when an add succeeds (briefly shown before close).
    pub last_added_to: Option<String>,
    /// Error from the last attempt.
    pub error: Option<String>,
}

/// Install the signal at app scope. Called from `App`.
pub fn install_add_to_playlist_state() {
    use_context_provider(|| Signal::new(AddToPlaylistState::default()));
}

#[component]
pub fn AddToPlaylistDialog() -> Element {
    let mut state = use_context::<Signal<AddToPlaylistState>>();
    let mut new_playlist = use_context::<Signal<NewPlaylistState>>();
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    if !state.read().open {
        return rsx! {};
    }

    let snap = state.read().clone();
    let close = move |_| state.set(AddToPlaylistState::default());

    let handle_for_resource = handle.clone();
    let playlists = use_resource(move || {
        let h = handle_for_resource.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            queries::playlists::list_all(h.library.pool()).await.ok()
        }
    });

    let open_new_playlist = move |_| {
        state.set(AddToPlaylistState::default());
        new_playlist.set(NewPlaylistState { open: true, ..NewPlaylistState::default() });
    };

    rsx! {
        div { class: "wizard-backdrop",
            div { class: "wizard wizard--narrow", role: "dialog", aria_modal: "true",
                onclick: move |evt| { evt.stop_propagation(); },
                header { class: "wizard__header",
                    h1 {
                        if let Some(title) = &snap.track_title {
                            "Add \"{title}\" to playlist"
                        } else {
                            "Add to playlist"
                        }
                    }
                    button { class: "wizard__close", onclick: close, "×" }
                }
                div { class: "wizard__body",
                    if let Some(name) = &snap.last_added_to {
                        p { class: "wizard__success", "✓ Added to {name}" }
                    }
                    if let Some(err) = &snap.error {
                        p { class: "wizard__error", "{err}" }
                    }
                    button {
                        class: "btn btn--primary add-to-playlist__new-btn",
                        onclick: open_new_playlist,
                        "+ New playlist"
                    }
                    div { class: "add-to-playlist__list",
                        match &*playlists.read_unchecked() {
                            Some(Some(rows)) => {
                                let manual: Vec<_> = rows.iter()
                                    .filter(|p| !p.is_smart())
                                    .cloned()
                                    .collect();
                                if manual.is_empty() {
                                    rsx! { p { class: "wizard__hint", "No playlists yet. Create one above." } }
                                } else {
                                    rsx! {
                                        for p in manual {
                                            // Inline the click handler per row so each
                                            // closure captures its own ids — avoids the
                                            // FnMut/Copy trap of sharing a multi-use callback.
                                            {
                                                let pid = p.id.clone();
                                                let pname = p.name.clone();
                                                let h = handle.clone();
                                                let mut state_h = state;
                                                let mut lib = library_signal;
                                                let on_pick = move |_| {
                                                    let Some(handle) = h.clone() else { return; };
                                                    let Some(track_id) = state_h.read().track_id.clone() else { return; };
                                                    let pid = pid.clone();
                                                    let pname = pname.clone();
                                                    dioxus::prelude::spawn(async move {
                                                        match queries::playlists::append_track(
                                                            handle.library.pool(),
                                                            &pid,
                                                            &track_id,
                                                        ).await {
                                                            Ok(_) => {
                                                                let next = lib.peek().version.wrapping_add(1);
                                                                lib.write().version = next;
                                                                state_h.write().last_added_to = Some(pname);
                                                                tokio::time::sleep(
                                                                    std::time::Duration::from_millis(700),
                                                                ).await;
                                                                state_h.set(AddToPlaylistState::default());
                                                            }
                                                            Err(e) => {
                                                                state_h.write().error = Some(format!("Failed: {e}"));
                                                            }
                                                        }
                                                    });
                                                };
                                                rsx! {
                                                    button {
                                                        class: "add-to-playlist__row",
                                                        onclick: on_pick,
                                                        span { class: "add-to-playlist__row-name", "{p.name}" }
                                                        span { class: "add-to-playlist__row-count", "{p.track_count} tracks" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            _ => rsx! { p { class: "wizard__hint", "Loading…" } }
                        }
                    }
                }
            }
        }
    }
}
