//! Artist detail page — bio, albums grid, top tracks.

use crate::app::use_app_handle;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::{Album, queries};

/// Artist detail page.
#[component]
pub fn ArtistDetail(id: String) -> Element {
    let handle = use_app_handle();
    let id_clone = id.clone();
    let artist = use_resource(move || {
        let h = handle.clone();
        let aid = id_clone.clone();
        async move {
            let h = h?;
            queries::artists::by_id(h.library.pool(), &aid).await.ok()
        }
    });

    let h2 = use_app_handle();
    let id_clone2 = id.clone();
    let albums = use_resource(move || {
        let h = h2.clone();
        let aid = id_clone2.clone();
        async move {
            let h = h?;
            queries::albums::by_artist(h.library.pool(), &aid).await.ok()
        }
    });

    rsx! {
        section { class: "artist-detail",
            header { class: "artist-detail__header",
                div { class: "artist-detail__photo" }
                div { class: "artist-detail__meta",
                    match &*artist.read_unchecked() {
                        Some(Some(a)) => rsx! {
                            h1 { class: "artist-detail__name", "{a.name}" }
                            if let Some(bio) = &a.bio {
                                p { class: "artist-detail__bio", "{bio}" }
                            }
                        },
                        _ => rsx! { h1 { class: "artist-detail__name", "Artist" } },
                    }
                }
            }
            div { class: "artist-detail__section",
                h2 { "Albums" }
                div { class: "albums-grid albums-grid--compact",
                    match &*albums.read_unchecked() {
                        Some(Some(rows)) => rsx! {
                            for a in rows.iter() {
                                AlbumTile { album: a.clone() }
                            }
                        },
                        _ => rsx! {},
                    }
                }
            }
        }
    }
}

#[component]
fn AlbumTile(album: Album) -> Element {
    let id = album.id.clone();
    rsx! {
        Link { to: Route::AlbumDetail { id: id.clone() }, class: "album-tile",
            div { class: "album-tile__cover" }
            div { class: "album-tile__title", "{album.title}" }
        }
    }
}
