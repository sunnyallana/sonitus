//! Manual playlist editor: rename, reorder, remove, change cover.

use dioxus::prelude::*;

/// Manual playlist editor.
#[component]
pub fn PlaylistEditor(id: String) -> Element {
    rsx! {
        section { class: "playlist-editor",
            h1 { "Edit playlist {id}" }
            label { class: "field",
                span { class: "field__label", "Name" }
                input { r#type: "text", class: "input" }
            }
            label { class: "field",
                span { class: "field__label", "Description" }
                textarea { class: "input input--multiline" }
            }
            div { class: "playlist-editor__tracks",
                h2 { "Tracks" }
                ul { class: "playlist-editor__list" }
            }
            div { class: "playlist-editor__actions",
                button { class: "btn btn--primary", "Save" }
                button { class: "btn btn--danger", "Delete playlist" }
            }
        }
    }
}
