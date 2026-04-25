//! Alphabetical artists list.

use crate::routes::Route;
use dioxus::prelude::*;

/// All artists, alphabetical.
#[component]
pub fn ArtistsList() -> Element {
    // Placeholder rows: a real implementation queries the library DB.
    rsx! {
        section { class: "artists-list",
            header { class: "artists-list__header",
                h1 { "Artists" }
                div { class: "artists-list__view-toggle",
                    button { class: "btn btn--ghost", "Grid" }
                    button { class: "btn btn--ghost", "List" }
                }
            }
            div { class: "artists-list__grid",
                for letter in 'A'..='Z' {
                    Link { to: Route::ArtistsList {}, class: "artist-tile",
                        span { class: "artist-tile__letter", "{letter}" }
                    }
                }
            }
        }
    }
}
