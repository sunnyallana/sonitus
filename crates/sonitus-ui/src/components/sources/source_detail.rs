//! Source detail — stats, last scan, rescan + disconnect controls.

use dioxus::prelude::*;

/// Source detail page.
#[component]
pub fn SourceDetail(id: String) -> Element {
    rsx! {
        section { class: "source-detail",
            h1 { "Source {id}" }
            dl { class: "source-detail__stats",
                dt { "Last scanned" }
                dd { "—" }
                dt { "Tracks indexed" }
                dd { "—" }
                dt { "Errors" }
                dd { "None" }
            }
            div { class: "source-detail__actions",
                button { class: "btn btn--primary", "Rescan now" }
                button { class: "btn btn--ghost", "Re-authenticate" }
                button { class: "btn btn--danger", "Disconnect" }
            }
        }
    }
}
