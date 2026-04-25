//! Genre cloud / browser.

use dioxus::prelude::*;

/// Genre cloud + filtered tracks below.
#[component]
pub fn GenreBrowser() -> Element {
    rsx! {
        section { class: "genre-browser",
            header { class: "genre-browser__header",
                h1 { "Genres" }
            }
            div { class: "genre-browser__cloud" }
            div { class: "genre-browser__results" }
        }
    }
}
