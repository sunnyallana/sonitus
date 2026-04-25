//! Smart playlist rule builder.

use dioxus::prelude::*;

/// Smart playlist rule editor.
#[component]
pub fn SmartPlaylistEditor(id: String) -> Element {
    rsx! {
        section { class: "smart-editor",
            h1 { "Smart playlist {id}" }
            div { class: "smart-editor__rules",
                p { "Build rules to filter your library." }
                ul { class: "smart-editor__list" }
                button { class: "btn btn--ghost", "+ Add rule" }
            }
            div { class: "smart-editor__preview",
                h2 { "Matching tracks (preview)" }
                ol { class: "smart-editor__preview-list" }
            }
        }
    }
}
