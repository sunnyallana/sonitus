//! Left sidebar — primary navigation on desktop and web.

use crate::routes::Route;
use dioxus::prelude::*;

/// Persistent left sidebar with primary nav links.
#[component]
pub fn Sidebar() -> Element {
    rsx! {
        nav { class: "sidebar", aria_label: "Primary navigation",
            div { class: "sidebar__brand",
                h1 { class: "sidebar__title", "Sonitus" }
            }
            ul { class: "sidebar__list",
                li { Link { to: Route::LibraryHome {}, class: "sidebar__link", "Library" } }
                li { Link { to: Route::ArtistsList {}, class: "sidebar__link", "Artists" } }
                li { Link { to: Route::AlbumsGrid {}, class: "sidebar__link", "Albums" } }
                li { Link { to: Route::TracksTable {}, class: "sidebar__link", "Tracks" } }
                li { Link { to: Route::GenreBrowser {}, class: "sidebar__link", "Genres" } }
                li { class: "sidebar__divider" }
                li { Link { to: Route::PlaylistsList {}, class: "sidebar__link", "Playlists" } }
                li { Link { to: Route::SourcesList {}, class: "sidebar__link", "Sources" } }
                li { Link { to: Route::DownloadsList {}, class: "sidebar__link", "Downloads" } }
                li { class: "sidebar__divider" }
                li { Link { to: Route::SettingsRoot {}, class: "sidebar__link", "Settings" } }
                li { Link { to: Route::PrivacyDashboard {}, class: "sidebar__link sidebar__link--privacy", "Privacy" } }
            }
        }
    }
}
