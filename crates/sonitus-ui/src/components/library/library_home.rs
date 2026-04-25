//! Library home page — recently added, recently played, smart playlists.

use crate::hooks::use_library::use_library;
use crate::routes::Route;
use dioxus::prelude::*;

/// Library home — landing page after first launch.
#[component]
pub fn LibraryHome() -> Element {
    let library = use_library();
    let state = library.read();

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
