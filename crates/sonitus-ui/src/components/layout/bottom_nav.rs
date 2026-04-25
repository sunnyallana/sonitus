//! Mobile bottom-nav: Library / Search / Queue / Settings.
//!
//! Only visible on narrow viewports — CSS handles the hide/show.

use crate::routes::Route;
use dioxus::prelude::*;

/// Mobile bottom navigation tab bar.
#[component]
pub fn BottomNav() -> Element {
    rsx! {
        nav { class: "bottom-nav", aria_label: "Mobile navigation",
            Link { to: Route::LibraryHome {}, class: "bottom-nav__tab",
                span { class: "bottom-nav__icon", "♫" }
                span { class: "bottom-nav__label", "Library" }
            }
            Link { to: Route::SearchResults { q: String::new() }, class: "bottom-nav__tab",
                span { class: "bottom-nav__icon", "⌕" }
                span { class: "bottom-nav__label", "Search" }
            }
            Link { to: Route::PlaylistsList {}, class: "bottom-nav__tab",
                span { class: "bottom-nav__icon", "≡" }
                span { class: "bottom-nav__label", "Playlists" }
            }
            Link { to: Route::SettingsRoot {}, class: "bottom-nav__tab",
                span { class: "bottom-nav__icon", "⚙" }
                span { class: "bottom-nav__label", "Settings" }
            }
        }
    }
}
