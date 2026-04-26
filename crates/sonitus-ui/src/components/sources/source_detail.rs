//! Source detail — stats, last scan, rescan + disconnect controls.
//!
//! - **Rescan now** triggers a synchronous-from-the-UI-pov scan of the
//!   source. Progress is read back from the source row's `scan_state`
//!   on every `library.version` bump.
//! - **Re-authenticate** is shown for OAuth-backed cloud kinds; clicking
//!   it kicks off the auth flow for the source kind. Hidden for local
//!   sources since they have no credentials.
//! - **Disconnect** deletes the source row (FK-cascade removes its
//!   tracks). Confirms first via a `Sure?` two-step button.

use crate::app::use_app_handle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;
use sonitus_core::library::scanner::Scanner;

/// Source detail page.
#[component]
pub fn SourceDetail(id: String) -> Element {
    let handle = use_app_handle();
    let mut library_signal = use_context::<Signal<LibraryState>>();
    let nav = navigator();

    let mut scanning = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut info = use_signal(|| Option::<String>::None);

    let id_for_resource = id.clone();
    let h_resource = handle.clone();
    let source = use_resource(move || {
        let h = h_resource.clone();
        let sid = id_for_resource.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            queries::sources::by_id(h.library.pool(), &sid).await.ok()
        }
    });

    // ── Rescan ───────────────────────────────────────────────────────────
    let id_rescan = id.clone();
    let h_rescan = handle.clone();
    let on_rescan = move |_| {
        let Some(h) = h_rescan.clone() else { return; };
        let sid = id_rescan.clone();
        let Some(provider) = h.sources.get(&sid).cloned() else {
            error.set(Some("This source is no longer registered. Try reopening the app.".into()));
            return;
        };
        scanning.set(true);
        info.set(None);
        error.set(None);
        let pool = h.library.pool().clone();
        dioxus::prelude::spawn(async move {
            // Discard progress events here; the source row itself is the
            // ground truth. We bump library.version on every progress so
            // the page re-fetches the row and the user sees state change.
            let (tx, mut rx) = tokio::sync::mpsc::channel(64);
            let scanner = Scanner::new(provider, pool);
            // Drain the progress channel into version bumps in parallel
            // with the scan itself so the user sees live feedback.
            let drain = async move {
                while rx.recv().await.is_some() {
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                }
            };
            let scan = async move { scanner.run(tx).await };
            let (result, _) = tokio::join!(scan, drain);
            scanning.set(false);
            match result {
                Ok(report) => {
                    info.set(Some(format!(
                        "Scan finished: {} added, {} updated, {} removed.",
                        report.tracks_added, report.tracks_updated, report.tracks_removed,
                    )));
                }
                Err(e) => error.set(Some(format!("Scan failed: {e}"))),
            }
            let next = library_signal.peek().version.wrapping_add(1);
            library_signal.write().version = next;
        });
    };

    // ── Disconnect ───────────────────────────────────────────────────────
    let id_delete = id.clone();
    let h_delete = handle.clone();
    let on_disconnect_first = move |_| confirm_disconnect.set(true);
    let on_disconnect_cancel = move |_| confirm_disconnect.set(false);
    let on_disconnect_confirm = move |_| {
        let Some(h) = h_delete.clone() else { return; };
        let sid = id_delete.clone();
        dioxus::prelude::spawn(async move {
            match queries::sources::delete(h.library.pool(), &sid).await {
                Ok(_) => {
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                    nav.replace(Route::SourcesList {});
                }
                Err(e) => error.set(Some(format!("Disconnect failed: {e}"))),
            }
        });
    };

    // ── Re-authenticate (placeholder for cloud OAuth) ───────────────────
    let on_reauth = move |_| {
        info.set(Some(
            "Re-authentication will live here once the cloud OAuth flow lands.".into(),
        ));
    };

    let snap_scanning = *scanning.read();
    let snap_confirm = *confirm_disconnect.read();
    let snap_error = error.read().clone();
    let snap_info = info.read().clone();

    rsx! {
        section { class: "source-detail",
            match &*source.read_unchecked() {
                Some(Some(s)) => {
                    let kind = s.kind.clone();
                    let needs_oauth = matches!(
                        kind.as_str(),
                        "google_drive" | "dropbox" | "onedrive"
                    );
                    rsx! {
                        h1 { "{s.name}" }
                        dl { class: "source-detail__stats",
                            dt { "Kind" }
                            dd { "{kind}" }
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

                        if let Some(msg) = &snap_info {
                            p { class: "wizard__success", "✓ {msg}" }
                        }
                        if let Some(err) = &snap_error {
                            p { class: "wizard__error", "{err}" }
                        }

                        div { class: "source-detail__actions",
                            button {
                                class: "btn btn--primary",
                                disabled: snap_scanning,
                                onclick: on_rescan,
                                if snap_scanning { "Scanning..." } else { "Rescan now" }
                            }
                            if needs_oauth {
                                button {
                                    class: "btn btn--ghost",
                                    onclick: on_reauth,
                                    "Re-authenticate"
                                }
                            }
                            if snap_confirm {
                                button {
                                    class: "btn btn--danger",
                                    onclick: on_disconnect_confirm,
                                    "Confirm disconnect"
                                }
                                button {
                                    class: "btn btn--ghost",
                                    onclick: on_disconnect_cancel,
                                    "Cancel"
                                }
                            } else {
                                button {
                                    class: "btn btn--danger",
                                    disabled: snap_scanning,
                                    onclick: on_disconnect_first,
                                    "Disconnect"
                                }
                            }
                        }
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
