//! Alphabetical artists list backed by the live DB.

use crate::app::use_app_handle;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::{Artist, queries};

/// All artists, alphabetical.
#[component]
pub fn ArtistsList() -> Element {
    let handle = use_app_handle();
    let artists = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            queries::artists::list_all(h.library.pool(), 1000, 0).await.ok()
        }
    });

    rsx! {
        section { class: "artists-list",
            header { class: "artists-list__header",
                h1 { "Artists" }
            }
            div { class: "artists-list__grid",
                match &*artists.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for a in rows.iter() {
                            ArtistTile { artist: a.clone() }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "artists-list__empty", "No artists yet." }
                    },
                    None => rsx! {
                        p { class: "artists-list__empty", "Loading…" }
                    },
                }
            }
        }
    }
}

#[component]
fn ArtistTile(artist: Artist) -> Element {
    let id = artist.id.clone();
    rsx! {
        Link { to: Route::ArtistDetail { id: id.clone() }, class: "artist-tile",
            div { class: "artist-tile__photo" }
            div { class: "artist-tile__name", "{artist.name}" }
        }
    }
}
