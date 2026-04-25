//! Add-source wizard.
//!
//! Currently only the **local** source kind is fully wired end-to-end.
//! Cloud sources sketch UI but their OAuth handshake is a follow-up.

use crate::app::use_app_handle;
use crate::orchestrator::AppHandle;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;
use sonitus_core::library::scanner::{ScanProgress, Scanner};
use sonitus_core::sources::SourceProvider;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Visibility + form state for the wizard. Provided as a Signal at the
/// SourcesList level so the "+ Add source" button and the wizard share state.
///
/// PartialEq is implemented manually because `Arc<AtomicBool>` doesn't
/// derive it; the cancel flag is transient and excluded from equality.
#[derive(Debug, Clone, Default)]
pub struct WizardState {
    /// Whether the dialog is currently shown.
    pub open: bool,
    /// Current step (0 = kind picker, 1 = configure, 2 = scanning).
    pub step: u8,
    /// Selected kind. Currently only "local" is fully wired.
    pub kind: String,
    /// User-provided name.
    pub name: String,
    /// User-provided path (local).
    pub path: String,
    /// Live progress while step == 2.
    pub progress: Option<ScanProgress>,
    /// Error string from the scan, if any.
    pub error: Option<String>,
    /// True once the scan has finished successfully.
    pub done: bool,
    /// Cancel flag the scanner reads cooperatively.
    pub cancel_flag: Option<Arc<AtomicBool>>,
    /// Human-readable description of what the scan task is doing right now.
    /// Updated by spawn_scan / scanner.run so the user can see where things
    /// are stuck if anything hangs.
    pub stage: String,
    /// Append-only log of stage transitions with timestamps. Shown in the
    /// debug panel.
    pub stage_log: Vec<String>,
    /// Pending request to start a scan. ConfigureLocal sets this; an effect
    /// in AddSourceWizard picks it up and spawns the scan in the wizard's
    /// own scope. Routing via state is required because tasks spawned from
    /// ConfigureLocal would die when ConfigureLocal unmounts on step change.
    pub scan_request: Option<ScanRequest>,
}

/// Marker that a scan should be started. Held briefly in `WizardState`
/// while AddSourceWizard's effect picks it up and clears it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanRequest {
    /// User-chosen display name for the source.
    pub name: String,
    /// Local filesystem path to scan.
    pub path: PathBuf,
}

impl PartialEq for WizardState {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open
            && self.step == other.step
            && self.kind == other.kind
            && self.name == other.name
            && self.path == other.path
            && self.progress == other.progress
            && self.error == other.error
            && self.done == other.done
            && self.stage == other.stage
            && self.stage_log.len() == other.stage_log.len()
            && self.scan_request == other.scan_request
        // cancel_flag intentionally not compared.
    }
}

/// Helper used by spawn_scan to atomically push a stage update + log line.
fn set_stage(wizard: &mut Signal<WizardState>, stage: impl Into<String>) {
    let stage = stage.into();
    let line = format!(
        "{} {}",
        chrono::Local::now().format("%H:%M:%S%.3f"),
        stage
    );
    tracing::info!(stage = %stage, "wizard: stage update");
    let mut w = wizard.write();
    w.stage = stage;
    w.stage_log.push(line);
    // Cap the log at 100 entries so memory stays bounded on very long scans.
    if w.stage_log.len() > 100 {
        let drop_count = w.stage_log.len() - 100;
        w.stage_log.drain(..drop_count);
    }
}

#[component]
pub fn AddSourceWizard() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    // The scan task lives here, in the wizard's scope, NOT in
    // ConfigureLocal. Reason: ConfigureLocal unmounts the moment
    // step transitions to 2, and Dioxus tasks are tied to their
    // calling scope — so a spawn from ConfigureLocal::on_start gets
    // cancelled before its body runs. AddSourceWizard stays mounted
    // for the entire duration of `wizard.open == true`, so spawns
    // started here survive the ConfigureLocal → Scanning swap.
    use_effect(move || {
        // Read scan_request to subscribe — re-runs when it changes.
        let request = wizard.read().scan_request.clone();
        let Some(req) = request else { return; };
        // Clear immediately so we never double-spawn.
        wizard.write().scan_request = None;
        let cancel = wizard
            .read()
            .cancel_flag
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let Some(handle) = handle.clone() else {
            wizard.write().error = Some("App handle unavailable".into());
            return;
        };
        spawn_scan(handle, req.name, req.path, wizard, library_signal, cancel);
    });

    if !wizard.read().open {
        return rsx! {};
    }
    let step = wizard.read().step;

    rsx! {
        div { class: "wizard-backdrop",
            onclick: move |_| { /* swallow backdrop clicks */ },
            div { class: "wizard", role: "dialog", aria_modal: "true",
                onclick: move |evt| { evt.stop_propagation(); },
                Header {}
                StepIndicator { active: step }
                div { class: "wizard__body",
                    match step {
                        0 => rsx! { KindPicker {} },
                        1 => rsx! { ConfigureLocal {} },
                        _ => rsx! { Scanning {} },
                    }
                }
            }
        }
    }
}

#[component]
fn Header() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    rsx! {
        header { class: "wizard__header",
            h1 { "Add a source" }
            button {
                class: "wizard__close",
                title: "Close",
                onclick: move |_| { wizard.set(WizardState::default()); },
                "×"
            }
        }
    }
}

#[component]
fn StepIndicator(active: u8) -> Element {
    let cls = |i: u8| {
        if i == active { "wizard__step wizard__step--active" }
        else if i < active { "wizard__step wizard__step--done" }
        else { "wizard__step" }
    };
    rsx! {
        ol { class: "wizard__steps",
            li { class: cls(0), span { class: "wizard__step-num", "1" } " Choose kind" }
            li { class: cls(1), span { class: "wizard__step-num", "2" } " Configure" }
            li { class: cls(2), span { class: "wizard__step-num", "3" } " Scan" }
        }
    }
}

#[component]
fn KindPicker() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let on_pick_local = move |_| {
        let mut w = wizard.write();
        w.kind = "local".to_string();
        w.step = 1;
        if w.name.is_empty() {
            w.name = "Local Music".to_string();
        }
        if w.path.is_empty() {
            if let Some(d) = dirs::audio_dir() {
                w.path = d.to_string_lossy().to_string();
            }
        }
    };

    rsx! {
        div { class: "wizard__intro",
            p { "Where does your music live? Pick a source kind to continue." }
        }
        div { class: "wizard__kinds",
            button { class: "kind-card", onclick: on_pick_local,
                span { class: "kind-card__icon", "📁" }
                strong { "Local folder" }
                span { class: "kind-card__sub", "Index music on this computer" }
            }
            DisabledKindCard { icon: "🟢", title: "Google Drive", sub: "Coming soon" }
            DisabledKindCard { icon: "🟧", title: "Amazon S3",    sub: "Coming soon" }
            DisabledKindCard { icon: "🖥️", title: "SMB / NAS",    sub: "Coming soon" }
            DisabledKindCard { icon: "🌐", title: "HTTP server",  sub: "Coming soon" }
            DisabledKindCard { icon: "🟦", title: "Dropbox",      sub: "Coming soon" }
            DisabledKindCard { icon: "🟩", title: "OneDrive",     sub: "Coming soon" }
        }
    }
}

#[component]
fn DisabledKindCard(icon: String, title: String, sub: String) -> Element {
    rsx! {
        button { class: "kind-card kind-card--disabled", disabled: true,
            span { class: "kind-card__icon", "{icon}" }
            strong { "{title}" }
            span { class: "kind-card__sub", "{sub}" }
        }
    }
}

#[component]
fn ConfigureLocal() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    let snapshot = wizard.read().clone();
    let name_val = snapshot.name.clone();
    let path_val = snapshot.path.clone();

    let on_name = move |evt: FormEvent| { wizard.write().name = evt.value(); };
    let on_path = move |evt: FormEvent| { wizard.write().path = evt.value(); };
    let on_back = move |_| { wizard.write().step = 0; };

    let on_browse = move |_| {
        // rfd is sync; spawn_blocking so we don't block Dioxus's local pool
        // while the OS dialog is up.
        let mut w = wizard;
        let initial = w.read().path.clone();
        dioxus::prelude::spawn(async move {
            let picked = tokio::task::spawn_blocking(move || {
                let mut dialog = rfd::FileDialog::new().set_title("Pick a music folder");
                if !initial.is_empty() {
                    dialog = dialog.set_directory(&initial);
                }
                dialog.pick_folder()
            })
            .await
            .ok()
            .flatten();
            if let Some(path) = picked {
                w.write().path = path.to_string_lossy().to_string();
            }
        });
    };

    // Hold onto these locally to silence the unused-variable warning; the
    // actual scan now runs from AddSourceWizard's effect, not from here.
    let _ = handle;
    let _ = library_signal;

    let on_start = move |_| {
        let snap = wizard.read().clone();
        let path = PathBuf::from(&snap.path);
        if !path.is_dir() {
            wizard.write().error = Some(format!(
                "'{}' is not a folder. Use Browse to pick one.",
                snap.path
            ));
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut w = wizard.write();
            w.step = 2;
            w.error = None;
            w.progress = None;
            w.done = false;
            w.cancel_flag = Some(cancel);
            // Hand off the request to AddSourceWizard's effect, which
            // owns the spawned task in a scope that survives our unmount.
            w.scan_request = Some(ScanRequest {
                name: snap.name.clone(),
                path,
            });
        }
        // ConfigureLocal will unmount on the next render now that step==2;
        // that's fine — the effect in AddSourceWizard is what drives the
        // scan from here on.
    };

    let start_disabled = name_val.trim().is_empty() || path_val.trim().is_empty();

    rsx! {
        div { class: "wizard__configure",
            label { class: "field",
                span { class: "field__label", "Name" }
                input {
                    class: "input",
                    r#type: "text",
                    value: "{name_val}",
                    oninput: on_name,
                    placeholder: "Local Music",
                }
            }
            label { class: "field",
                span { class: "field__label", "Folder" }
                div { class: "field__path-row",
                    input {
                        class: "input field__path-input",
                        r#type: "text",
                        value: "{path_val}",
                        oninput: on_path,
                        placeholder: r"C:\Users\you\Music",
                    }
                    button { class: "btn btn--ghost", onclick: on_browse, "Browse..." }
                }
                span { class: "field__hint",
                    "Tip: paste a path or use Browse to pick a folder. The default is your OS Music folder."
                }
            }

            if let Some(err) = &snapshot.error {
                p { class: "wizard__error", "{err}" }
            }

            div { class: "wizard__footer",
                button { class: "btn btn--ghost", onclick: on_back, "← Back" }
                button {
                    class: "btn btn--primary",
                    disabled: start_disabled,
                    onclick: on_start,
                    "Start scan →"
                }
            }
        }
    }
}

#[component]
fn Scanning() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let snapshot = wizard.read().clone();

    let close = move |_| { wizard.set(WizardState::default()); };

    let cancel = move |_| {
        if let Some(flag) = &wizard.read().cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    };

    rsx! {
        div { class: "wizard__scanning",
            if let Some(err) = &snapshot.error {
                h2 { "Scan failed" }
                pre { class: "wizard__error", "{err}" }
                div { class: "wizard__footer",
                    button { class: "btn btn--primary", onclick: close, "Close" }
                }
            } else if snapshot.done {
                h2 { class: "wizard__success", "✓ Done" }
                if let Some(p) = &snapshot.progress {
                    div { class: "wizard__stats",
                        div { class: "wizard__stat",
                            span { class: "wizard__stat-num", "{p.tracks_indexed}" }
                            span { class: "wizard__stat-label", "tracks indexed" }
                        }
                        div { class: "wizard__stat",
                            span { class: "wizard__stat-num", "{p.files_seen}" }
                            span { class: "wizard__stat-label", "files seen" }
                        }
                        if p.files_failed > 0 {
                            div { class: "wizard__stat wizard__stat--warn",
                                span { class: "wizard__stat-num", "{p.files_failed}" }
                                span { class: "wizard__stat-label", "failed" }
                            }
                        }
                    }
                }
                p { class: "wizard__hint",
                    "Visit the Tracks, Albums, or Artists pages to browse your library."
                }
                div { class: "wizard__footer",
                    button { class: "btn btn--primary", onclick: close, "Done" }
                }
            } else {
                h2 { "Scanning..." }
                if !snapshot.stage.is_empty() {
                    p { class: "wizard__stage", "→ {snapshot.stage}" }
                }
                if let Some(p) = &snapshot.progress {
                    div { class: "wizard__stats",
                        div { class: "wizard__stat",
                            span { class: "wizard__stat-num", "{p.tracks_indexed}" }
                            span { class: "wizard__stat-label", "tracks indexed" }
                        }
                        div { class: "wizard__stat",
                            span { class: "wizard__stat-num", "{p.files_seen}" }
                            span { class: "wizard__stat-label", "files seen" }
                        }
                        if p.files_failed > 0 {
                            div { class: "wizard__stat wizard__stat--warn",
                                span { class: "wizard__stat-num", "{p.files_failed}" }
                                span { class: "wizard__stat-label", "failed" }
                            }
                        }
                    }
                    if let Some(cur) = &p.current_file {
                        p { class: "wizard__current",
                            span { class: "wizard__current-label", "Current: " }
                            span { class: "wizard__current-path", "{cur}" }
                        }
                    }
                }
                div { class: "wizard__progress",
                    div { class: "wizard__progress-bar wizard__progress-bar--indeterminate" }
                }

                // Debug log: shows the last several stage transitions with
                // timestamps. Lets users diagnose stuck scans without
                // needing access to the dx terminal.
                details { class: "wizard__debug",
                    summary { "Debug log ({snapshot.stage_log.len()} entries)" }
                    pre { class: "wizard__debug-log",
                        for line in snapshot.stage_log.iter() {
                            "{line}\n"
                        }
                    }
                }

                div { class: "wizard__footer",
                    button { class: "btn btn--ghost", onclick: cancel, "Cancel scan" }
                }
            }
        }
    }
}

fn spawn_scan(
    handle: AppHandle,
    name: String,
    path: PathBuf,
    wizard: Signal<WizardState>,
    library_signal: Signal<LibraryState>,
    cancel: Arc<AtomicBool>,
) {
    let mut library_signal = library_signal;
    let mut wizard_outer = wizard; // Signals are Copy; rename for clarity vs the inner `wizard_for_progress`.
    dioxus::prelude::spawn(async move {
        set_stage(&mut wizard_outer, "spawn_scan: task started");

        let id = uuid::Uuid::new_v4().to_string();
        let config_json = serde_json::json!({
            "path": path.to_string_lossy().to_string(),
        })
        .to_string();

        set_stage(
            &mut wizard_outer,
            format!("inserting sources row (id={})", &id[..8]),
        );
        if let Err(e) = queries::sources::insert(
            handle.library.vault(),
            &id,
            &name,
            "local",
            &config_json,
            None,
        )
        .await
        {
            set_stage(&mut wizard_outer, format!("INSERT FAILED: {e}"));
            wizard_outer.write().error = Some(format!("Failed to save source: {e}"));
            return;
        }
        set_stage(&mut wizard_outer, "sources row inserted; building LocalSource");

        let source: Arc<dyn SourceProvider> = Arc::new(
            sonitus_core::sources::local::LocalSource::new(id.clone(), name.clone(), path.clone()),
        );
        let scanner = Scanner::new(source, handle.library.pool().clone());
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanProgress>(64);

        set_stage(&mut wizard_outer, "spawning progress drain task");
        let mut wizard_for_progress = wizard_outer;
        let cancel_for_progress = cancel.clone();
        let progress_task = dioxus::prelude::spawn(async move {
            tracing::info!("progress drain task: started");
            while let Some(p) = rx.recv().await {
                if cancel_for_progress.load(Ordering::Relaxed) {
                    tracing::info!("progress drain task: cancelled by user");
                    break;
                }
                wizard_for_progress.write().progress = Some(p);
            }
            tracing::info!("progress drain task: rx closed, exiting");
        });

        set_stage(&mut wizard_outer, "calling Scanner::run (will set scan_state, read pre-existing tracks, then walk)");
        let run_result = scanner.run(tx).await;
        set_stage(&mut wizard_outer, format!("Scanner::run returned: ok={}", run_result.is_ok()));

        match run_result {
            Ok(report) => {
                if cancel.load(Ordering::Relaxed) {
                    wizard_outer.write().error = Some("Scan was cancelled.".into());
                } else {
                    set_stage(
                        &mut wizard_outer,
                        format!(
                            "scan complete: +{} added / {} updated / {} removed / {} failed in {}ms",
                            report.tracks_added,
                            report.tracks_updated,
                            report.tracks_removed,
                            report.files_failed,
                            report.duration_ms,
                        ),
                    );
                    wizard_outer.write().done = true;
                }
            }
            Err(e) => {
                set_stage(&mut wizard_outer, format!("SCAN FAILED: {e}"));
                wizard_outer.write().error = Some(format!("Scan failed: {e}"));
            }
        }
        let _ = progress_task;

        set_stage(&mut wizard_outer, "refreshing library summary");
        if let Ok(summary) = handle.library.summary().await {
            let mut s = library_signal.write();
            s.track_count = summary.tracks;
            s.album_count = summary.albums;
            s.artist_count = summary.artists;
            s.playlist_count = summary.playlists;
        }
        if let Ok(sources) = queries::sources::list_all(handle.library.pool()).await {
            library_signal.write().sources = sources;
        }
        set_stage(&mut wizard_outer, "spawn_scan: task done");
    });
}
