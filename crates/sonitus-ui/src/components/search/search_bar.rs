//! Top-bar search input. Submitting navigates to /search?q=…

use crate::routes::Route;
use dioxus::prelude::*;

/// Top-bar search input. Type → press Enter → routes to the results page.
#[component]
pub fn SearchBar() -> Element {
    let mut query = use_signal(String::new);
    let nav = navigator();

    rsx! {
        form { class: "search-bar", role: "search",
            onsubmit: move |evt| {
                evt.prevent_default();
                let q = query.read().trim().to_string();
                if q.is_empty() { return; }
                nav.push(Route::SearchResults { q });
            },
            input { class: "search-bar__input",
                r#type: "search",
                placeholder: "Search tracks, albums, artists... (press Enter)",
                aria_label: "Search the library",
                value: "{query}",
                oninput: move |evt| query.set(evt.value()),
            }
            kbd { class: "search-bar__kbd", "Ctrl+K" }
        }
    }
}
