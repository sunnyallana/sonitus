//! Source detail — stats, last scan, rescan + disconnect controls.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Source detail page.
#[component]
pub fn SourceDetail(id: String) -> Element {
    let handle = use_app_handle();
    let id_clone = id.clone();
    let source = use_resource(move || {
        let h = handle.clone();
        let sid = id_clone.clone();
        async move {
            let h = h?;
            queries::sources::by_id(h.library.pool(), &sid).await.ok()
        }
    });

    rsx! {
        section { class: "source-detail",
            match &*source.read_unchecked() {
                Some(Some(s)) => rsx! {
                    h1 { "{s.name}" }
                    dl { class: "source-detail__stats",
                        dt { "Kind" }
                        dd { "{s.kind}" }
                        dt { "Tracks indexed" }
                        dd { "{s.track_count}" }
                        dt { "State" }
                        dd { "{s.scan_state}" }
                        dt { "Last scanned" }
                        dd {
                            match s.last_scanned_at {
                                Some(ts) => format_ts(ts),
                                None => "never".into(),
                            }
                        }
                        if let Some(err) = &s.last_error {
                            dt { "Last error" }
                            dd { class: "source-detail__error", "{err}" }
                        }
                    }
                    div { class: "source-detail__actions",
                        button { class: "btn btn--primary", "Rescan now" }
                        button { class: "btn btn--ghost", "Re-authenticate" }
                        button { class: "btn btn--danger", "Disconnect" }
                    }
                },
                _ => rsx! { p { "Source not found." } },
            }
        }
    }
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "—".into())
}
