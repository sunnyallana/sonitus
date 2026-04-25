//! Reusable playlist card with cover collage.

use crate::routes::Route;
use dioxus::prelude::*;

/// Playlist tile used in grids.
#[component]
pub fn PlaylistCard(id: String, name: String, track_count: i64) -> Element {
    rsx! {
        Link { to: Route::PlaylistDetail { id: id.clone() }, class: "playlist-card",
            div { class: "playlist-card__cover" }
            div { class: "playlist-card__meta",
                div { class: "playlist-card__name", "{name}" }
                div { class: "playlist-card__count", "{track_count} tracks" }
            }
        }
    }
}
