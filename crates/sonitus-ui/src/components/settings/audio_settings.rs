//! Audio settings page.

use dioxus::prelude::*;

/// Audio settings.
#[component]
pub fn AudioSettings() -> Element {
    rsx! {
        section { class: "settings-page",
            h1 { "Audio" }
            label { class: "field",
                span { class: "field__label", "Output device" }
                select { class: "select",
                    option { value: "", "System default" }
                }
            }
            label { class: "field",
                span { class: "field__label", "ReplayGain" }
                select { class: "select",
                    option { value: "off", "Off" }
                    option { value: "track", selected: true, "Track" }
                    option { value: "album", "Album" }
                }
            }
            label { class: "field",
                span { class: "field__label", "Crossfade (seconds)" }
                input { r#type: "range", min: "0", max: "12", step: "1", value: "0", class: "range" }
            }
            label { class: "field field--inline",
                input { r#type: "checkbox", checked: true }
                span { "Gapless playback" }
            }
            label { class: "field",
                span { class: "field__label", "Buffer size" }
                select { class: "select",
                    option { value: "small", "Small (lowest latency)" }
                    option { value: "medium", selected: true, "Medium" }
                    option { value: "large", "Large (smoothest)" }
                }
            }
        }
    }
}
