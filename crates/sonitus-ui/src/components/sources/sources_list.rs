//! List of connected sources.

use crate::hooks::use_library::use_library;
use crate::routes::Route;
use dioxus::prelude::*;

/// Sources page — list each source, with rescan/disable buttons.
#[component]
pub fn SourcesList() -> Element {
    let library = use_library();
    let state = library.read();

    rsx! {
        section { class: "sources-list",
            header { class: "sources-list__header",
                h1 { "Sources" }
                button { class: "btn btn--primary", "+ Add source" }
            }
            ul { class: "sources-list__items",
                for src in state.sources.iter() {
                    li { class: "source-row", key: "{src.id}",
                        div { class: "source-row__name",
                            Link { to: Route::SourceDetail { id: src.id.clone() }, "{src.name}" }
                        }
                        div { class: "source-row__kind", "{src.kind}" }
                        div { class: "source-row__count", "{src.track_count} tracks" }
                        div { class: "source-row__state", "{src.scan_state}" }
                    }
                }
            }
            if state.sources.is_empty() {
                p { class: "sources-list__empty",
                    "No sources yet. Add one to start indexing music."
                }
            }
        }
    }
}
