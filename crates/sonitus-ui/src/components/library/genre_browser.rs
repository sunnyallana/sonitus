//! Genre cloud / browser backed by the live DB.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Genre cloud + filtered tracks below.
#[component]
pub fn GenreBrowser() -> Element {
    let handle = use_app_handle();
    let genres = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            queries::tracks::genres(h.library.pool()).await.ok()
        }
    });

    rsx! {
        section { class: "genre-browser",
            header { class: "genre-browser__header",
                h1 { "Genres" }
            }
            div { class: "genre-browser__cloud",
                match &*genres.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for (g, n) in rows.iter() {
                            span { class: "genre-chip",
                                "{g} "
                                span { class: "genre-chip__count", "({n})" }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "genre-browser__empty", "No genre tags found." }
                    },
                    None => rsx! { p { "Loading…" } },
                }
            }
            div { class: "genre-browser__results" }
        }
    }
}
