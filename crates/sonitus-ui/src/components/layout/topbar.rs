//! Top bar with search input, scan progress indicator, settings shortcut.

use crate::components::search::search_bar::SearchBar;
use crate::hooks::use_library::use_library;
use crate::routes::Route;
use dioxus::prelude::*;

/// Sticky top bar at the top of every page.
#[component]
pub fn Topbar() -> Element {
    let library = use_library();
    let scanning = library.is_any_scanning();

    rsx! {
        header { class: "topbar",
            div { class: "topbar__search",
                SearchBar {}
            }
            div { class: "topbar__indicator",
                if scanning {
                    span { class: "topbar__scan-pill", "Scanning sources..." }
                }
            }
            div { class: "topbar__actions",
                Link { to: Route::SettingsRoot {}, class: "topbar__settings",
                    aria_label: "Open settings",
                    "⚙"
                }
            }
        }
    }
}
