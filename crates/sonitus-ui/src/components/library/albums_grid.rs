//! Albums grid — grid/list, sortable.

use dioxus::prelude::*;

/// Albums grid view.
#[component]
pub fn AlbumsGrid() -> Element {
    rsx! {
        section { class: "albums-grid-page",
            header { class: "albums-grid-page__header",
                h1 { "Albums" }
                div { class: "albums-grid-page__filters",
                    select { class: "select",
                        option { value: "year", "By year" }
                        option { value: "alpha", "Alphabetical" }
                        option { value: "artist", "By artist" }
                    }
                }
            }
            div { class: "albums-grid" }
        }
    }
}
