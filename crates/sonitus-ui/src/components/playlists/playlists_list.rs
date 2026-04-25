//! Grid of playlist cards.

use crate::app::use_app_handle;
use crate::components::playlists::playlist_card::PlaylistCard;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Playlists landing page — grid of cards + create button.
#[component]
pub fn PlaylistsList() -> Element {
    let handle = use_app_handle();
    let playlists = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            queries::playlists::list_all(h.library.pool()).await.ok()
        }
    });

    rsx! {
        section { class: "playlists-list",
            header { class: "playlists-list__header",
                h1 { "Playlists" }
                div { class: "playlists-list__actions",
                    button { class: "btn btn--primary", "+ New playlist" }
                    button { class: "btn btn--ghost", "+ New smart playlist" }
                    button { class: "btn btn--ghost", "Import M3U8" }
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
                        p { class: "playlists-list__empty", "No playlists yet." }
                    },
                    None => rsx! {
                        p { class: "playlists-list__empty", "Loading…" }
                    },
                }
            }
        }
    }
}
