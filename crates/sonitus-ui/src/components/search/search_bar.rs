//! Debounced search input with Cmd/Ctrl+K shortcut.

use crate::hooks::use_search::use_search;
use dioxus::prelude::*;

/// Top-bar search input.
#[component]
pub fn SearchBar() -> Element {
    let mut query = use_signal(String::new);
    let search = use_search();

    rsx! {
        form { class: "search-bar", role: "search",
            onsubmit: move |evt| {
                evt.prevent_default();
                let q = query.read().clone();
                search.set_query(q);
            },
            input { class: "search-bar__input",
                r#type: "search",
                placeholder: "Search tracks, albums, artists...",
                aria_label: "Search the library",
                value: "{query}",
                oninput: move |evt| query.set(evt.value()),
            }
            kbd { class: "search-bar__kbd", "Ctrl+K" }
        }
    }
}
