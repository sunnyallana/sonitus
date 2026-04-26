//! Add-source wizard.
//!
//! Each source kind has its own configure form (step 1). On submit, the
//! form persists the source row + encrypted credentials and hands off to
//! the shared scanning step (2). After a successful scan the source is
//! visible in the database; a restart is currently required for it to
//! appear in `AppHandle::sources` for live playback (the source registry
//! is built at boot).
//!
//! Currently fully wired:
//! - Local folder
//! - HTTP server (no auth)
//! - Amazon S3 / MinIO / R2 (access key + secret key)
//!
//! Disabled with a "needs OAuth setup" hint:
//! - Google Drive, Dropbox, OneDrive — these require provider-issued
//!   client_id + client_secret pairs that aren't bundled with Sonitus
//!   for privacy reasons. Wire them up by setting the env vars
//!   `SONITUS_GOOGLE_CLIENT_ID`, etc., and rebuilding.
//!
//! Disabled because the feature flag is off in this build:
//! - SMB / NAS — turn on the `smb` feature in `sonitus-core` to enable.

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
#[derive(Debug, Clone, Default)]
pub struct WizardState {
    /// Whether the dialog is currently shown.
    pub open: bool,
    /// Current step (0 = kind picker, 1 = configure, 2 = scanning).
    pub step: u8,
    /// Selected kind (`local` | `http` | `s3`).
    pub kind: String,
    /// User-provided name.
    pub name: String,

    // ── Per-kind config fields. Only the fields relevant to `kind` are
    //    used; the rest stay empty. Keeping them flat (rather than an
    //    enum) keeps the per-kind component code straightforward and
    //    avoids cloning a big enum payload on every keystroke.
    /// Local: filesystem path. HTTP: base URL.
    pub path: String,
    /// S3: bucket name.
    pub s3_bucket: String,
    /// S3: optional key prefix.
    pub s3_prefix: String,
    /// S3: region name.
    pub s3_region: String,
    /// S3: optional endpoint URL (MinIO/R2).
    pub s3_endpoint: String,
    /// S3: access key.
    pub s3_access_key: String,
    /// S3: secret key.
    pub s3_secret_key: String,

    /// Live progress while step == 2.
    pub progress: Option<ScanProgress>,
    /// Error string from the scan, if any.
    pub error: Option<String>,
    /// True once the scan has finished successfully.
    pub done: bool,
    /// Cancel flag the scanner reads cooperatively.
    pub cancel_flag: Option<Arc<AtomicBool>>,
    /// Human-readable description of what the scan task is doing right now.
    pub stage: String,
    /// Append-only log of stage transitions with timestamps.
    pub stage_log: Vec<String>,
    /// Pending request to start a scan.
    pub scan_request: Option<ScanRequest>,
}

/// Marker that a scan should be started. Held briefly while the wizard
/// effect picks it up and clears it.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanRequest {
    /// Local folder scan.
    Local {
        /// Display name for the source.
        name: String,
        /// Folder to scan.
        path: PathBuf,
    },
    /// Plain HTTP base URL.
    Http {
        /// Display name.
        name: String,
        /// Base URL (parsed before insertion).
        base_url: String,
    },
    /// Amazon S3 / S3-compatible bucket.
    S3 {
        /// Display name.
        name: String,
        /// Bucket name.
        bucket: String,
        /// Optional prefix to scope into a sub-folder.
        prefix: String,
        /// AWS region.
        region: String,
        /// Optional endpoint URL (MinIO, R2).
        endpoint_url: Option<String>,
        /// Access key.
        access_key: String,
        /// Secret key.
        secret_key: String,
    },
}

impl PartialEq for WizardState {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open
            && self.step == other.step
            && self.kind == other.kind
            && self.name == other.name
            && self.path == other.path
            && self.s3_bucket == other.s3_bucket
            && self.s3_prefix == other.s3_prefix
            && self.s3_region == other.s3_region
            && self.s3_endpoint == other.s3_endpoint
            && self.s3_access_key == other.s3_access_key
            // intentionally not comparing s3_secret_key — secrets must
            // not influence equality (would let a noisy diff-engine peek)
            && self.progress == other.progress
            && self.error == other.error
            && self.done == other.done
            && self.stage == other.stage
            && self.stage_log.len() == other.stage_log.len()
            && self.scan_request == other.scan_request
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

    use_effect(move || {
        let request = wizard.read().scan_request.clone();
        let Some(req) = request else { return; };
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
        spawn_scan(handle, req, wizard, library_signal, cancel);
    });

    if !wizard.read().open {
        return rsx! {};
    }
    let snap = wizard.read().clone();

    rsx! {
        div { class: "wizard-backdrop",
            onclick: move |_| { /* swallow backdrop clicks */ },
            div { class: "wizard", role: "dialog", aria_modal: "true",
                onclick: move |evt| { evt.stop_propagation(); },
                Header {}
                StepIndicator { active: snap.step }
                div { class: "wizard__body",
                    match snap.step {
                        0 => rsx! { KindPicker {} },
                        1 => match snap.kind.as_str() {
                            "http" => rsx! { ConfigureHttp {} },
                            "s3" => rsx! { ConfigureS3 {} },
                            _ => rsx! { ConfigureLocal {} },
                        },
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
        if w.name.is_empty() { w.name = "Local Music".to_string(); }
        if w.path.is_empty() {
            if let Some(d) = dirs::audio_dir() {
                w.path = d.to_string_lossy().to_string();
            }
        }
    };
    let on_pick_http = move |_| {
        let mut w = wizard.write();
        w.kind = "http".to_string();
        w.step = 1;
        if w.name.is_empty() { w.name = "HTTP server".to_string(); }
    };
    let on_pick_s3 = move |_| {
        let mut w = wizard.write();
        w.kind = "s3".to_string();
        w.step = 1;
        if w.name.is_empty() { w.name = "S3 bucket".to_string(); }
        if w.s3_region.is_empty() { w.s3_region = "us-east-1".to_string(); }
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
            button { class: "kind-card", onclick: on_pick_http,
                span { class: "kind-card__icon", "🌐" }
                strong { "HTTP server" }
                span { class: "kind-card__sub", "Index a directory listing" }
            }
            button { class: "kind-card", onclick: on_pick_s3,
                span { class: "kind-card__icon", "🟧" }
                strong { "S3 / R2 / MinIO" }
                span { class: "kind-card__sub", "Bucket with audio files" }
            }
            DisabledKindCard {
                icon: "🟢",
                title: "Google Drive",
                sub: "Needs OAuth credentials".to_string(),
            }
            DisabledKindCard {
                icon: "🟦",
                title: "Dropbox",
                sub: "Needs OAuth credentials".to_string(),
            }
            DisabledKindCard {
                icon: "🟩",
                title: "OneDrive",
                sub: "Needs OAuth credentials".to_string(),
            }
            DisabledKindCard {
                icon: "🖥️",
                title: "SMB / NAS",
                sub: "Enable the `smb` feature".to_string(),
            }
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
    let snap = wizard.read().clone();
    let name_val = snap.name.clone();
    let path_val = snap.path.clone();

    let on_name = move |evt: FormEvent| { wizard.write().name = evt.value(); };
    let on_path = move |evt: FormEvent| { wizard.write().path = evt.value(); };
    let on_back = move |_| { wizard.write().step = 0; };

    let on_browse = move |_| {
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
        let mut w = wizard.write();
        w.step = 2;
        w.error = None;
        w.progress = None;
        w.done = false;
        w.cancel_flag = Some(cancel);
        w.scan_request = Some(ScanRequest::Local {
            name: snap.name.clone(),
            path,
        });
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
            }
            if let Some(err) = &snap.error {
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
fn ConfigureHttp() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let snap = wizard.read().clone();

    let on_name = move |evt: FormEvent| { wizard.write().name = evt.value(); };
    let on_url = move |evt: FormEvent| { wizard.write().path = evt.value(); };
    let on_back = move |_| { wizard.write().step = 0; };

    let on_start = move |_| {
        let snap = wizard.read().clone();
        let url = snap.path.trim().to_string();
        if url::Url::parse(&url).is_err() {
            wizard.write().error = Some(
                "That doesn't look like a URL. Try https://example.com/music/.".into()
            );
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let mut w = wizard.write();
        w.step = 2;
        w.error = None;
        w.progress = None;
        w.done = false;
        w.cancel_flag = Some(cancel);
        w.scan_request = Some(ScanRequest::Http {
            name: snap.name.clone(),
            base_url: url,
        });
    };

    let start_disabled = snap.name.trim().is_empty() || snap.path.trim().is_empty();

    rsx! {
        div { class: "wizard__configure",
            label { class: "field",
                span { class: "field__label", "Name" }
                input {
                    class: "input",
                    r#type: "text",
                    value: "{snap.name}",
                    oninput: on_name,
                    placeholder: "HTTP server",
                }
            }
            label { class: "field",
                span { class: "field__label", "Base URL" }
                input {
                    class: "input",
                    r#type: "url",
                    value: "{snap.path}",
                    oninput: on_url,
                    placeholder: "https://example.com/music/",
                }
                span { class: "field__hint",
                    "Sonitus expects directory listings at this URL. Use a trailing slash."
                }
            }
            if let Some(err) = &snap.error {
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
fn ConfigureS3() -> Element {
    let mut wizard = use_context::<Signal<WizardState>>();
    let snap = wizard.read().clone();

    let on_name = move |evt: FormEvent| { wizard.write().name = evt.value(); };
    let on_bucket = move |evt: FormEvent| { wizard.write().s3_bucket = evt.value(); };
    let on_prefix = move |evt: FormEvent| { wizard.write().s3_prefix = evt.value(); };
    let on_region = move |evt: FormEvent| { wizard.write().s3_region = evt.value(); };
    let on_endpoint = move |evt: FormEvent| { wizard.write().s3_endpoint = evt.value(); };
    let on_access = move |evt: FormEvent| { wizard.write().s3_access_key = evt.value(); };
    let on_secret = move |evt: FormEvent| { wizard.write().s3_secret_key = evt.value(); };
    let on_back = move |_| { wizard.write().step = 0; };

    let on_start = move |_| {
        let snap = wizard.read().clone();
        if snap.s3_bucket.trim().is_empty() {
            wizard.write().error = Some("Bucket name can't be empty.".into());
            return;
        }
        if snap.s3_access_key.trim().is_empty() || snap.s3_secret_key.is_empty() {
            wizard.write().error = Some("Access key and secret key are required.".into());
            return;
        }
        let endpoint = if snap.s3_endpoint.trim().is_empty() {
            None
        } else {
            Some(snap.s3_endpoint.trim().to_string())
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let mut w = wizard.write();
        w.step = 2;
        w.error = None;
        w.progress = None;
        w.done = false;
        w.cancel_flag = Some(cancel);
        w.scan_request = Some(ScanRequest::S3 {
            name: snap.name.clone(),
            bucket: snap.s3_bucket.trim().to_string(),
            prefix: snap.s3_prefix.trim().to_string(),
            region: snap.s3_region.trim().to_string(),
            endpoint_url: endpoint,
            access_key: snap.s3_access_key.trim().to_string(),
            secret_key: snap.s3_secret_key.clone(),
        });
    };

    let start_disabled = snap.name.trim().is_empty()
        || snap.s3_bucket.trim().is_empty()
        || snap.s3_access_key.trim().is_empty()
        || snap.s3_secret_key.is_empty();

    rsx! {
        div { class: "wizard__configure",
            label { class: "field",
                span { class: "field__label", "Name" }
                input { class: "input", r#type: "text", value: "{snap.name}", oninput: on_name }
            }
            label { class: "field",
                span { class: "field__label", "Bucket" }
                input { class: "input", r#type: "text", value: "{snap.s3_bucket}", oninput: on_bucket, placeholder: "my-music-bucket" }
            }
            label { class: "field",
                span { class: "field__label", "Prefix (optional)" }
                input { class: "input", r#type: "text", value: "{snap.s3_prefix}", oninput: on_prefix, placeholder: "audio/" }
            }
            label { class: "field",
                span { class: "field__label", "Region" }
                input { class: "input", r#type: "text", value: "{snap.s3_region}", oninput: on_region, placeholder: "us-east-1" }
            }
            label { class: "field",
                span { class: "field__label", "Endpoint URL (optional, for MinIO/R2)" }
                input { class: "input", r#type: "text", value: "{snap.s3_endpoint}", oninput: on_endpoint, placeholder: "https://s3.example.com" }
            }
            label { class: "field",
                span { class: "field__label", "Access key" }
                input { class: "input", r#type: "text", value: "{snap.s3_access_key}", oninput: on_access, autocomplete: "off" }
            }
            label { class: "field",
                span { class: "field__label", "Secret key" }
                input { class: "input", r#type: "password", value: "{snap.s3_secret_key}", oninput: on_secret, autocomplete: "off" }
                span { class: "field__hint",
                    "Stored encrypted at rest with the vault key — never written to disk in plaintext."
                }
            }
            if let Some(err) = &snap.error {
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
                    "Visit the Tracks, Albums, or Artists pages to browse your library. "
                    "Restart Sonitus once to enable streaming from cloud sources."
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
    request: ScanRequest,
    wizard: Signal<WizardState>,
    library_signal: Signal<LibraryState>,
    cancel: Arc<AtomicBool>,
) {
    let mut library_signal = library_signal;
    let mut wizard_outer = wizard;
    dioxus::prelude::spawn(async move {
        set_stage(&mut wizard_outer, "spawn_scan: task started");
        let id = uuid::Uuid::new_v4().to_string();

        // ── Build provider + insert source row, per-kind ─────────────────
        let provider_result = build_provider(&handle, &id, &request).await;
        let (source, name) = match provider_result {
            Ok(p) => p,
            Err(e) => {
                set_stage(&mut wizard_outer, format!("provider build failed: {e}"));
                wizard_outer.write().error = Some(e);
                return;
            }
        };

        set_stage(&mut wizard_outer, "running scanner");
        let scanner = Scanner::new(source, handle.library.pool().clone());
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanProgress>(64);

        let mut wizard_for_progress = wizard_outer;
        let cancel_for_progress = cancel.clone();
        let progress_task = dioxus::prelude::spawn(async move {
            while let Some(p) = rx.recv().await {
                if cancel_for_progress.load(Ordering::Relaxed) { break; }
                wizard_for_progress.write().progress = Some(p);
            }
        });

        let run_result = scanner.run(tx).await;
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
        let _ = name;

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
        let next = library_signal.peek().version.wrapping_add(1);
        library_signal.write().version = next;
    });
}

/// Persist the source row + credentials and construct the matching
/// `SourceProvider`. Returns `(provider, name)`.
async fn build_provider(
    handle: &AppHandle,
    id: &str,
    request: &ScanRequest,
) -> Result<(Arc<dyn SourceProvider>, String), String> {
    use sonitus_core::crypto::types::SourceCredential;

    match request {
        ScanRequest::Local { name, path } => {
            let config_json = serde_json::json!({
                "path": path.to_string_lossy().to_string(),
            })
            .to_string();
            queries::sources::insert(
                handle.library.vault(),
                id,
                name,
                "local",
                &config_json,
                None,
            )
            .await
            .map_err(|e| format!("Failed to save source: {e}"))?;
            let provider: Arc<dyn SourceProvider> = Arc::new(
                sonitus_core::sources::local::LocalSource::new(
                    id.to_string(),
                    name.clone(),
                    path.clone(),
                ),
            );
            Ok((provider, name.clone()))
        }
        ScanRequest::Http { name, base_url } => {
            let url = url::Url::parse(base_url)
                .map_err(|e| format!("Invalid URL: {e}"))?;
            let config_json = serde_json::json!({
                "base_url": url.to_string(),
            })
            .to_string();
            queries::sources::insert(
                handle.library.vault(),
                id,
                name,
                "http",
                &config_json,
                None,
            )
            .await
            .map_err(|e| format!("Failed to save source: {e}"))?;
            let provider: Arc<dyn SourceProvider> = Arc::new(
                sonitus_core::sources::http::HttpSource::new(
                    id.to_string(),
                    name.clone(),
                    url,
                    handle.audit.clone(),
                ),
            );
            Ok((provider, name.clone()))
        }
        #[cfg(feature = "s3")]
        ScanRequest::S3 {
            name, bucket, prefix, region, endpoint_url, access_key, secret_key,
        } => {
            let config_json = serde_json::json!({
                "bucket": bucket,
                "prefix": prefix,
                "region": region,
                "endpoint_url": endpoint_url,
            })
            .to_string();
            let creds = SourceCredential {
                kind: "s3".to_string(),
                primary: access_key.clone(),
                secondary: Some(secret_key.clone()),
                expires_at: None,
            };
            queries::sources::insert(
                handle.library.vault(),
                id,
                name,
                "s3",
                &config_json,
                Some(&creds),
            )
            .await
            .map_err(|e| format!("Failed to save source: {e}"))?;
            let provider = sonitus_core::sources::s3::S3Source::new(
                id.to_string(),
                name.clone(),
                bucket.clone(),
                prefix.clone(),
                access_key.clone(),
                secret_key.clone(),
                region.clone(),
                endpoint_url.clone(),
                handle.audit.clone(),
            )
            .await
            .map_err(|e| format!("Couldn't connect to S3: {e}"))?;
            let provider: Arc<dyn SourceProvider> = Arc::new(provider);
            Ok((provider, name.clone()))
        }
        #[cfg(not(feature = "s3"))]
        ScanRequest::S3 { .. } => Err(
            "S3 support isn't compiled into this build. Enable the `s3` feature.".to_string()
        ),
    }
}
