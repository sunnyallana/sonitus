//! Storage settings.

use dioxus::prelude::*;

/// Storage / cache settings.
#[component]
pub fn StorageSettings() -> Element {
    rsx! {
        section { class: "settings-page",
            h1 { "Storage" }
            label { class: "field",
                span { class: "field__label", "Cache size limit (GB)" }
                input { r#type: "number", min: "1", max: "1000", value: "10", class: "input" }
            }
            div { class: "field",
                span { class: "field__label", "Cache used" }
                p { class: "field__value", "0 MB" }
            }
            div { class: "field",
                button { class: "btn btn--danger", "Clear cache" }
            }
            label { class: "field",
                span { class: "field__label", "Download location" }
                input { r#type: "text", placeholder: "(platform default)", class: "input" }
            }
            div { class: "field",
                span { class: "field__label", "Database" }
                p { class: "field__value", "library.db" }
                button { class: "btn btn--ghost", "Backup database" }
            }
        }
    }
}
