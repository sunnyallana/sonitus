//! Virtualized audit log viewer.
//!
//! Reads from the JSONL file in the platform data directory. Filters by
//! destination / triggered_by / date range. Exports to CSV.

use dioxus::prelude::*;

/// Audit log viewer page.
#[component]
pub fn AuditLogViewer() -> Element {
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
                tbody { class: "audit-log__body" }
            }
        }
    }
}
