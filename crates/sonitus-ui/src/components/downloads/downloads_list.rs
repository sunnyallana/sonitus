//! Active + completed + failed downloads.

use crate::hooks::use_downloads::use_downloads;
use dioxus::prelude::*;

/// Downloads page.
#[component]
pub fn DownloadsList() -> Element {
    let downloads = use_downloads();
    let items = downloads.read();

    rsx! {
        section { class: "downloads-list",
            header { class: "downloads-list__header",
                h1 { "Downloads" }
                div { class: "downloads-list__actions",
                    button { class: "btn btn--ghost", "Pause all" }
                    button { class: "btn btn--ghost", "Clear completed" }
                }
            }
            if items.is_empty() {
                p { class: "downloads-list__empty", "No downloads in flight." }
            }
            ul { class: "downloads-list__items",
                for d in items {
                    li { class: "download-row", key: "{d.id}",
                        div { class: "download-row__title", "{d.track_title}" }
                        div { class: "download-row__progress",
                            div { class: "progress-bar",
                                div { class: "progress-bar__fill", style: "width: {(d.progress * 100.0)}%" }
                            }
                        }
                        div { class: "download-row__status", "{d.status}" }
                    }
                }
            }
        }
    }
}
