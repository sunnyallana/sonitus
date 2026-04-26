//! Active + completed + failed downloads.
//!
//! Live state comes from the `Signal<Vec<DownloadItem>>` mirror that the
//! orchestrator populates from `DownloadUpdate` events. The two header
//! buttons run against the `DownloadManager` directly via `AppHandle`.

use crate::app::use_app_handle;
use crate::hooks::use_downloads::use_downloads;
use dioxus::prelude::*;

/// Downloads page.
#[component]
pub fn DownloadsList() -> Element {
    let downloads = use_downloads();
    let handle = use_app_handle();
    let items = downloads.read();

    let any_active = items.iter().any(|d| d.status == "downloading" || d.status == "queued");
    let any_terminal = items.iter().any(|d| {
        matches!(d.status.as_str(), "done" | "failed" | "cancelled")
    });

    let h_pause = handle.clone();
    let active_ids: Vec<String> = items
        .iter()
        .filter(|d| d.status == "downloading" || d.status == "queued")
        .map(|d| d.id.clone())
        .collect();
    let on_pause_all = move |_| {
        let Some(h) = h_pause.clone() else { return; };
        let ids = active_ids.clone();
        dioxus::prelude::spawn(async move {
            for id in ids {
                let _ = h.downloads.pause(&id).await;
            }
        });
    };

    let h_clear = handle.clone();
    let on_clear_completed = move |_| {
        let Some(h) = h_clear.clone() else { return; };
        dioxus::prelude::spawn(async move {
            let _ = h.downloads.purge_terminal().await;
        });
    };

    rsx! {
        section { class: "downloads-list",
            header { class: "downloads-list__header",
                h1 { "Downloads" }
                div { class: "downloads-list__actions",
                    button {
                        class: "btn btn--ghost",
                        disabled: !any_active,
                        onclick: on_pause_all,
                        "Pause all"
                    }
                    button {
                        class: "btn btn--ghost",
                        disabled: !any_terminal,
                        onclick: on_clear_completed,
                        "Clear completed"
                    }
                }
            }
            if items.is_empty() {
                p { class: "downloads-list__empty", "No downloads in flight." }
            }
            ul { class: "downloads-list__items",
                for d in items.iter() {
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
