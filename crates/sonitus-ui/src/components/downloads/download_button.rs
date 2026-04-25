//! Reusable download button with state indicator.

use dioxus::prelude::*;

/// Download button with state indicator (idle / queued / downloading / cached).
#[component]
pub fn DownloadButton(track_id: String) -> Element {
    rsx! {
        button { class: "download-button",
            title: "Download for offline listening",
            aria_label: "Download track {track_id}",
            "↓"
        }
    }
}
