//! Albums grid — grid/list, sortable.

use crate::app::use_app_handle;
use crate::components::library::cover_art::CoverArt;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::{Album, queries};

/// Albums grid view.
#[component]
pub fn AlbumsGrid() -> Element {
    let handle = use_app_handle();
    let albums = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            queries::albums::list(h.library.pool(), None, 500, 0).await.ok()
        }
    });

    rsx! {
        section { class: "albums-grid-page",
            header { class: "albums-grid-page__header",
                h1 { "Albums" }
            }
            div { class: "albums-grid",
                match &*albums.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for a in rows.iter() {
                            AlbumTile { album: a.clone() }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "albums-grid__empty", "No albums yet." }
                    },
                    None => rsx! {
                        p { class: "albums-grid__empty", "Loading…" }
                    },
                }
            }
        }
    }
}

#[component]
fn AlbumTile(album: Album) -> Element {
    let id = album.id.clone();
    let year = album.year.map(|y| y.to_string()).unwrap_or_default();
    rsx! {
        Link { to: Route::AlbumDetail { id: id.clone() }, class: "album-tile",
            CoverArt { album_id: Some(album.id.clone()), size_class: "cover-art--lg".to_string() }
            div { class: "album-tile__title", "{album.title}" }
            div { class: "album-tile__year", "{year}" }
        }
    }
}
