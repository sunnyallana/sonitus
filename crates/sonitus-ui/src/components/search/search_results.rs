//! Grouped search results: Tracks / Albums / Artists / Playlists.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::SearchKind;

/// Search results page.
#[component]
pub fn SearchResults(q: String) -> Element {
    let handle = use_app_handle();
    let q_clone = q.clone();
    let results = use_resource(move || {
        let h = handle.clone();
        let q = q_clone.clone();
        async move {
            if q.is_empty() { return Some(Vec::new()); }
            let h = h?;
            sonitus_core::library::search(h.library.pool(), &q, 100).await.ok()
        }
    });

    rsx! {
        section { class: "search-results",
            header { class: "search-results__header",
                h1 { "Search: \"{q}\"" }
            }
            match &*results.read_unchecked() {
                None => rsx! { p { class: "search-results__loading", "Searching…" } },
                Some(items) if items.is_empty() && !q.is_empty() => rsx! {
                    p { class: "search-results__empty", "No results." }
                },
                Some(items) if items.is_empty() => rsx! {
                    p { class: "search-results__empty", "Type something to search." }
                },
                Some(items) => rsx! {
                    ResultGroup { items: items.clone(), kind: SearchKind::Track, label: "Tracks".to_string() }
                    ResultGroup { items: items.clone(), kind: SearchKind::Album, label: "Albums".to_string() }
                    ResultGroup { items: items.clone(), kind: SearchKind::Artist, label: "Artists".to_string() }
                },
            }
        }
    }
}

#[component]
fn ResultGroup(
    items: Vec<sonitus_core::library::SearchResult>,
    kind: SearchKind,
    label: String,
) -> Element {
    let filtered: Vec<_> = items.into_iter().filter(|r| r.kind == kind).collect();
    if filtered.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "result-group",
            h2 { class: "result-group__title", "{label}" }
            ul { class: "result-group__list",
                for item in filtered {
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
