//! Grid of playlist cards.

use dioxus::prelude::*;

/// Playlists landing page — grid of cards + create button.
#[component]
pub fn PlaylistsList() -> Element {
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
            div { class: "playlists-grid" }
        }
    }
}
