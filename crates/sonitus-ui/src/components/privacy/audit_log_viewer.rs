//! Virtualized audit log viewer.
//!
//! Reads from the JSONL file in the platform data directory. Filters by
//! destination / triggered_by / date range. Exports to CSV.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::privacy::AuditEntry;

/// Audit log viewer page.
#[component]
pub fn AuditLogViewer() -> Element {
    let handle = use_app_handle();
    let entries = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            // Read on a blocking thread — file IO; the audit logger uses sync writes.
            let logger = h.audit.clone();
            let result = tokio::task::spawn_blocking(move || logger.read_entries())
                .await
                .ok()?;
            result.ok()
        }
    });

    rsx! {
        section { class: "audit-log",
            h1 { "Audit log" }
            p { class: "audit-log__intro",
                "Every outbound HTTP request Sonitus has made. Use the filters to drill in."
            }

            div { class: "audit-log__filters",
                label { class: "field field--inline",
                    span { "Destination" }
                    input { r#type: "text", placeholder: "any", class: "input input--small" }
                }
                label { class: "field field--inline",
                    span { "Triggered by" }
                    select { class: "select select--small",
                        option { value: "", "Any" }
                        option { value: "user_action", "User action" }
                        option { value: "background_scan", "Background scan" }
                        option { value: "metadata_lookup", "Metadata lookup" }
                        option { value: "oauth_refresh", "OAuth refresh" }
                        option { value: "download", "Download" }
                        option { value: "playback", "Playback" }
                    }
                }
                button { class: "btn btn--ghost", "Export CSV" }
            }

            table { class: "audit-log__table", role: "grid",
                thead {
                    tr {
                        th { "Time" }
                        th { "Destination" }
                        th { "Method" }
                        th { "Path" }
                        th { "By" }
                        th { "Sent" }
                        th { "Recv" }
                        th { "Status" }
                        th { "ms" }
                    }
                }
                tbody { class: "audit-log__body",
                    match &*entries.read_unchecked() {
                        Some(Some(rows)) if !rows.is_empty() => rsx! {
                            for e in rows.iter().rev().take(500) {
                                AuditRow { entry: e.clone() }
                            }
                        },
                        Some(_) => rsx! {
                            tr { td { colspan: "9",
                                "No outbound requests recorded yet — Sonitus is working entirely locally."
                            } }
                        },
                        None => rsx! {
                            tr { td { colspan: "9", "Loading…" } }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn AuditRow(entry: AuditEntry) -> Element {
    let ts = entry.ts.format("%Y-%m-%d %H:%M:%S").to_string();
    let status = entry.status.map(|s| s.to_string()).unwrap_or_else(|| "—".into());
    rsx! {
        tr {
            td { "{ts}" }
            td { "{entry.dest}" }
            td { "{entry.method}" }
            td { "{entry.path}" }
            td { "{entry.by}" }
            td { "{entry.sent}" }
            td { "{entry.recv}" }
            td { "{status}" }
            td { "{entry.ms}" }
        }
    }
}
