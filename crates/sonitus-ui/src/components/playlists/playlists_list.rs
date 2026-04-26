//! Grid of playlist cards.

use crate::app::use_app_handle;
use crate::components::playlists::new_playlist_dialog::NewPlaylistState;
use crate::components::playlists::playlist_card::PlaylistCard;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Playlists landing page — grid of cards + create button.
#[component]
pub fn PlaylistsList() -> Element {
    // Provide the dialog state at this scope.
    // The dialog state is provided at App scope; just read it here.
    let mut dialog_state = use_context::<Signal<NewPlaylistState>>();

    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();
    let playlists = use_resource(move || {
        let h = handle.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            queries::playlists::list_all(h.library.pool()).await.ok()
        }
    });

    let open_new = move |_| {
        dialog_state.set(NewPlaylistState { open: true, ..NewPlaylistState::default() });
    };

    rsx! {
        section { class: "playlists-list",
            header { class: "playlists-list__header",
                h1 { "Playlists" }
                div { class: "playlists-list__actions",
                    button { class: "btn btn--primary", onclick: open_new, "+ New playlist" }
                    Link {
                        to: Route::SmartPlaylistEditor { id: "new".to_string() },
                        class: "btn btn--ghost",
                        "+ New smart playlist"
                    }
                }
            }
            div { class: "playlists-grid",
                match &*playlists.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for p in rows.iter() {
                            PlaylistCard {
                                id: p.id.clone(),
                                name: p.name.clone(),
                                track_count: p.track_count,
                            }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "playlists-list__empty", "No playlists yet. Click + New playlist to make one." }
                    },
                    None => rsx! {
                        p { class: "playlists-list__empty", "Loading…" }
                    },
                }
            }
        }
    }
}
