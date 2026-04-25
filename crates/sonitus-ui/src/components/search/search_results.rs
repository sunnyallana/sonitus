//! Grouped search results: Tracks / Albums / Artists / Playlists.

use crate::hooks::use_search::use_search;
use dioxus::prelude::*;
use sonitus_core::library::SearchKind;

/// Search results page.
#[component]
pub fn SearchResults(q: String) -> Element {
    let search = use_search();
    let state = search.read();

    rsx! {
        section { class: "search-results",
            header { class: "search-results__header",
                h1 { "Search: \"{q}\"" }
            }
            if state.loading {
                p { class: "search-results__loading", "Searching..." }
            }
            if state.results.is_empty() && !state.loading {
                p { class: "search-results__empty",
                    if q.is_empty() { "Type something to search." } else { "No results." }
                }
            }
            ResultGroup { kind: SearchKind::Track, label: "Tracks".to_string() }
            ResultGroup { kind: SearchKind::Album, label: "Albums".to_string() }
            ResultGroup { kind: SearchKind::Artist, label: "Artists".to_string() }
        }
    }
}

#[component]
fn ResultGroup(kind: SearchKind, label: String) -> Element {
    let search = use_search();
    let state = search.read();
    let items: Vec<_> = state.results.into_iter().filter(|r| r.kind == kind).collect();
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "result-group",
            h2 { class: "result-group__title", "{label}" }
            ul { class: "result-group__list",
                for item in items {
                    li { class: "result-row", key: "{item.id}",
                        span { class: "result-row__title", "{item.title}" }
                        if let Some(sub) = &item.subtitle {
                            span { class: "result-row__sub", "{sub}" }
                        }
                    }
                }
            }
        }
    }
}
