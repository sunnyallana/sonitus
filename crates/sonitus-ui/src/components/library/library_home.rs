//! Library home page — recently added, recently played, smart playlists.

use crate::app::use_app_handle;
use crate::hooks::use_library::use_library;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Library home — landing page after first launch.
#[component]
pub fn LibraryHome() -> Element {
    let library = use_library();
    let state = library.read();

    let handle = use_app_handle();
    let recent = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            queries::tracks::recently_added(h.library.pool(), 12).await.ok()
        }
    });

    rsx! {
        section { class: "library-home",
            header { class: "library-home__header",
                h1 { "Your library" }
                p { class: "library-home__summary",
                    "{state.track_count} tracks · {state.album_count} albums · {state.artist_count} artists"
                }
            }
            div { class: "library-home__grid",
                NavCard { title: "Tracks", to: Route::TracksTable {}, count: state.track_count }
                NavCard { title: "Albums", to: Route::AlbumsGrid {}, count: state.album_count }
                NavCard { title: "Artists", to: Route::ArtistsList {}, count: state.artist_count }
                NavCard { title: "Playlists", to: Route::PlaylistsList {}, count: state.playlist_count }
            }

            section { class: "library-home__section",
                h2 { "Recently added" }
                ul { class: "library-home__recent",
                    match &*recent.read_unchecked() {
                        Some(Some(rows)) if !rows.is_empty() => rsx! {
                            for t in rows.iter() {
                                li { class: "recent-row", key: "{t.id}",
                                    span { class: "recent-row__title", "{t.title}" }
                                }
                            }
                        },
                        _ => rsx! {},
                    }
                }
            }

            if state.track_count == 0 {
                EmptyState {}
            }
        }
    }
}

#[component]
fn NavCard(title: String, to: Route, count: u64) -> Element {
    rsx! {
        Link { to: to, class: "nav-card",
            div { class: "nav-card__title", "{title}" }
            div { class: "nav-card__count", "{count}" }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    rsx! {
        div { class: "empty-state",
            h2 { "Add a source to get started" }
            p { "Sonitus indexes music wherever you keep it. Add a folder, a Google Drive, an S3 bucket, or your home NAS." }
            Link { to: Route::SourcesList {}, class: "empty-state__cta", "Add a source" }
        }
    }
}
