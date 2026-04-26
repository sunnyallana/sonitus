//! Virtualized audit log viewer.
//!
//! Reads from the JSONL file in the platform data directory. Filters by
//! destination substring and trigger source. Exports the *filtered* set
//! to CSV via the native save dialog.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::privacy::{AuditEntry, TriggerSource};

/// Audit log viewer page.
#[component]
pub fn AuditLogViewer() -> Element {
    let handle = use_app_handle();
    let mut dest_filter = use_signal(String::new);
    let mut trigger_filter = use_signal(|| Option::<TriggerSource>::None);
    let mut export_status = use_signal(|| Option::<String>::None);

    let entries = use_resource(move || {
        let h = handle.clone();
        async move {
            let h = h?;
            let logger = h.audit.clone();
            let result = tokio::task::spawn_blocking(move || logger.read_entries())
                .await
                .ok()?;
            result.ok()
        }
    });

    let on_dest = move |evt: FormEvent| dest_filter.set(evt.value());
    let on_trigger = move |evt: FormEvent| {
        let v = evt.value();
        trigger_filter.set(parse_trigger(&v));
    };

    // Apply current filters to the loaded log. We compute this twice (once
    // for the table, once for the CSV button) — cheap, and avoids stuffing
    // it into another resource.
    let filtered: Vec<AuditEntry> = match entries.read_unchecked().as_ref() {
        Some(Some(rows)) => {
            let dest = dest_filter.read().to_lowercase();
            let trig = *trigger_filter.read();
            rows.iter()
                .filter(|e| {
                    let dest_ok = dest.is_empty() || e.dest.to_lowercase().contains(&dest);
                    let trig_ok = trig.map(|t| e.by == t).unwrap_or(true);
                    dest_ok && trig_ok
                })
                .cloned()
                .collect()
        }
        _ => Vec::new(),
    };

    let on_export = {
        let filtered = filtered.clone();
        move |_| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let Some(dest) = rfd::FileDialog::new()
                    .set_file_name("sonitus-audit.csv")
                    .add_filter("CSV", &["csv"])
                    .save_file()
                else {
                    return;
                };
                match write_csv(&dest, &filtered) {
                    Ok(_) => export_status.set(Some(format!(
                        "Exported {} rows to {}", filtered.len(), dest.display()
                    ))),
                    Err(e) => export_status.set(Some(format!("Export failed: {e}"))),
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = filtered;
            }
        }
    };

    let snap_dest = dest_filter.read().clone();
    let snap_trigger = *trigger_filter.read();
    let snap_status = export_status.read().clone();

    rsx! {
        section { class: "audit-log",
            h1 { "Audit log" }
            p { class: "audit-log__intro",
                "Every outbound HTTP request Sonitus has made. Use the filters to drill in."
            }

            div { class: "audit-log__filters",
                label { class: "field field--inline",
                    span { "Destination" }
                    input {
                        r#type: "text",
                        placeholder: "any",
                        class: "input input--small",
                        value: "{snap_dest}",
                        oninput: on_dest,
                    }
                }
                label { class: "field field--inline",
                    span { "Triggered by" }
                    select {
                        class: "select select--small",
                        value: trigger_value(snap_trigger),
                        onchange: on_trigger,
                        option { value: "", "Any" }
                        option { value: "user_action", "User action" }
                        option { value: "background_scan", "Background scan" }
                        option { value: "metadata_lookup", "Metadata lookup" }
                        option { value: "oauth_refresh", "OAuth refresh" }
                        option { value: "download", "Download" }
                        option { value: "playback", "Playback" }
                    }
                }
                button {
                    class: "btn btn--ghost",
                    disabled: filtered.is_empty(),
                    onclick: on_export,
                    "Export CSV"
                }
            }

            if let Some(msg) = &snap_status {
                p { class: "wizard__success", "{msg}" }
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
                    match (&*entries.read_unchecked(), filtered.is_empty()) {
                        (Some(Some(rows)), _) if rows.is_empty() => rsx! {
                            tr { td { colspan: "9",
                                "No outbound requests recorded yet — Sonitus is working entirely locally."
                            } }
                        },
                        (Some(Some(_)), true) => rsx! {
                            tr { td { colspan: "9", "No matches for the current filter." } }
                        },
                        (Some(Some(_)), false) => rsx! {
                            for e in filtered.iter().rev().take(500) {
                                AuditRow { entry: e.clone() }
                            }
                        },
                        (Some(None), _) => rsx! {
                            tr { td { colspan: "9", "Couldn't read audit log." } }
                        },
                        (None, _) => rsx! {
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

fn parse_trigger(s: &str) -> Option<TriggerSource> {
    Some(match s {
        "user_action" => TriggerSource::UserAction,
        "background_scan" => TriggerSource::BackgroundScan,
        "metadata_lookup" => TriggerSource::MetadataLookup,
        "oauth_refresh" => TriggerSource::OauthRefresh,
        "download" => TriggerSource::Download,
        "playback" => TriggerSource::Playback,
        _ => return None,
    })
}

fn trigger_value(t: Option<TriggerSource>) -> &'static str {
    match t {
        None => "",
        Some(TriggerSource::UserAction) => "user_action",
        Some(TriggerSource::BackgroundScan) => "background_scan",
        Some(TriggerSource::MetadataLookup) => "metadata_lookup",
        Some(TriggerSource::OauthRefresh) => "oauth_refresh",
        Some(TriggerSource::Download) => "download",
        Some(TriggerSource::Playback) => "playback",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_csv(path: &std::path::Path, rows: &[AuditEntry]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "ts,dest,method,path,by,sent,recv,status,ms,error")?;
    for r in rows {
        let ts = r.ts.format("%Y-%m-%dT%H:%M:%SZ");
        let status = r.status.map(|s| s.to_string()).unwrap_or_default();
        let err = r.error.as_deref().unwrap_or("");
        writeln!(
            f,
            "{ts},{},{},{},{},{},{},{status},{},{}",
            csv_escape(&r.dest),
            csv_escape(&r.method),
            csv_escape(&r.path),
            r.by,
            r.sent,
            r.recv,
            r.ms,
            csv_escape(err),
        )?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
