//! Artist detail page — bio, albums grid, top tracks.

use dioxus::prelude::*;

/// Artist detail page.
#[component]
pub fn ArtistDetail(id: String) -> Element {
    rsx! {
        section { class: "artist-detail",
            header { class: "artist-detail__header",
                div { class: "artist-detail__photo" }
                div { class: "artist-detail__meta",
                    h1 { class: "artist-detail__name", "Artist {id}" }
                    p { class: "artist-detail__sub", "Albums and tracks" }
                }
            }
            div { class: "artist-detail__section",
                h2 { "Albums" }
                div { class: "albums-grid albums-grid--compact" }
            }
            div { class: "artist-detail__section",
                h2 { "Top tracks" }
                ol { class: "top-tracks" }
            }
        }
    }
}
