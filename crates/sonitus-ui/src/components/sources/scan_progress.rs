//! Live scan progress meter.

use dioxus::prelude::*;

/// Live scan progress widget.
#[component]
pub fn ScanProgress(source_id: String) -> Element {
    rsx! {
        div { class: "scan-progress", aria_live: "polite",
            div { class: "scan-progress__bar",
                div { class: "scan-progress__fill", style: "width: 0%" }
            }
            div { class: "scan-progress__stats",
                "Files seen: 0 · Tracks indexed: 0 · Errors: 0"
            }
        }
    }
}
