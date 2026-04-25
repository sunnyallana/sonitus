//! Album detail — cover art, metadata, full track list.

use dioxus::prelude::*;

/// Album detail page.
#[component]
pub fn AlbumDetail(id: String) -> Element {
    rsx! {
        section { class: "album-detail",
            header { class: "album-detail__header",
                div { class: "album-detail__cover" }
                div { class: "album-detail__meta",
                    h1 { class: "album-detail__title", "Album {id}" }
                    p { class: "album-detail__artist", "Artist" }
                    p { class: "album-detail__year", "Year · Genre" }
                    div { class: "album-detail__actions",
                        button { class: "btn btn--primary", "Play album" }
                        button { class: "btn btn--ghost", "Shuffle" }
                        button { class: "btn btn--ghost", "Add to queue" }
                        button { class: "btn btn--ghost", "Download" }
                    }
                }
            }
            ol { class: "album-detail__tracks" }
        }
    }
}
