//! Playlist detail — header + track list + edit button.

use dioxus::prelude::*;

/// Playlist detail page.
#[component]
pub fn PlaylistDetail(id: String) -> Element {
    rsx! {
        section { class: "playlist-detail",
            header { class: "playlist-detail__header",
                div { class: "playlist-detail__cover" }
                div { class: "playlist-detail__meta",
                    h1 { class: "playlist-detail__title", "Playlist {id}" }
                    div { class: "playlist-detail__sub", "0 tracks · 0:00" }
                    div { class: "playlist-detail__actions",
                        button { class: "btn btn--primary", "Play" }
                        button { class: "btn btn--ghost", "Shuffle" }
                        button { class: "btn btn--ghost", "Edit" }
                        button { class: "btn btn--ghost", "Export M3U8" }
                    }
                }
            }
            ol { class: "playlist-detail__tracks" }
        }
    }
}
