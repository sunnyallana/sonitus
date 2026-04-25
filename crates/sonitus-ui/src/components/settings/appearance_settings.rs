//! Appearance settings.

use dioxus::prelude::*;

/// Appearance settings.
#[component]
pub fn AppearanceSettings() -> Element {
    rsx! {
        section { class: "settings-page",
            h1 { "Appearance" }
            label { class: "field",
                span { class: "field__label", "Theme" }
                select { class: "select",
                    option { value: "system", "Match system" }
                    option { value: "dark", selected: true, "Dark" }
                    option { value: "light", "Light" }
                }
            }
            label { class: "field",
                span { class: "field__label", "Accent color" }
                input { r#type: "color", value: "#1DB954", class: "color-picker" }
            }
            label { class: "field",
                span { class: "field__label", "Font size" }
                select { class: "select",
                    option { value: "small", "Small" }
                    option { value: "medium", selected: true, "Medium" }
                    option { value: "large", "Large" }
                }
            }
            label { class: "field",
                span { class: "field__label", "Library default view" }
                select { class: "select",
                    option { value: "grid", selected: true, "Grid" }
                    option { value: "list", "List" }
                }
            }
        }
    }
}
