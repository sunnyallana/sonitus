//! Dialog for creating a new manual playlist.
//!
//! Opened from the Playlists page header. Submits via
//! `queries::playlists::create_manual` and bumps `library_state.version`
//! so the playlists list refreshes.

use crate::app::use_app_handle;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// State signal for the new-playlist dialog. Provided at app scope so any
/// component can open it (the queue panel uses this to "save the queue").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NewPlaylistState {
    /// Whether the dialog is open.
    pub open: bool,
    /// Current name field value.
    pub name: String,
    /// Optional description field value.
    pub description: String,
    /// Error message from the last attempt, if any.
    pub error: Option<String>,
    /// Track IDs to append to the playlist immediately after creation.
    /// Used by "Save queue as playlist" so the user gets one combined
    /// flow instead of "create empty playlist, then bulk-add tracks."
    pub seed_track_ids: Vec<String>,
}

/// The dialog component itself. Renders nothing when state.open is false.
#[component]
pub fn NewPlaylistDialog() -> Element {
    let mut state = use_context::<Signal<NewPlaylistState>>();
    let mut library_signal = use_context::<Signal<LibraryState>>();
    let handle = use_app_handle();

    if !state.read().open {
        return rsx! {};
    }
    let snap = state.read().clone();

    let close = move |_| state.set(NewPlaylistState::default());

    let on_name = move |evt: FormEvent| { state.write().name = evt.value(); };
    let on_desc = move |evt: FormEvent| { state.write().description = evt.value(); };

    let on_submit = move |evt: FormEvent| {
        evt.prevent_default();
        let Some(handle) = handle.clone() else { return; };
        let snap = state.read().clone();
        let name = snap.name.trim().to_string();
        if name.is_empty() {
            state.write().error = Some("Name can't be empty.".into());
            return;
        }
        let desc = snap.description.trim().to_string();
        let desc_opt = if desc.is_empty() { None } else { Some(desc) };
        let seed_ids = snap.seed_track_ids.clone();
        dioxus::prelude::spawn(async move {
            let pool = handle.library.pool();
            let created = match queries::playlists::create_manual(
                pool,
                &name,
                desc_opt.as_deref(),
            )
            .await
            {
                Ok(p) => p,
                Err(e) => {
                    state.write().error = Some(format!("Failed: {e}"));
                    return;
                }
            };

            // Append any seed tracks (e.g. from "Save queue as playlist").
            // Errors here don't undo creation — we surface the partial
            // failure but keep the new playlist around since the user
            // probably still wants it.
            for tid in &seed_ids {
                if let Err(e) =
                    queries::playlists::append_track(pool, &created.id, tid).await
                {
                    state.write().error = Some(format!(
                        "Created '{name}', but adding tracks failed: {e}"
                    ));
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                    return;
                }
            }

            state.set(NewPlaylistState::default());
            let next = library_signal.peek().version.wrapping_add(1);
            library_signal.write().version = next;
        });
    };

    rsx! {
        div { class: "wizard-backdrop",
            form {
                class: "wizard",
                role: "dialog",
                aria_modal: "true",
                onsubmit: on_submit,
                onclick: move |evt| { evt.stop_propagation(); },
                header { class: "wizard__header",
                    h1 {
                        if snap.seed_track_ids.is_empty() {
                            "New playlist"
                        } else {
                            "Save queue as playlist"
                        }
                    }
                    button { r#type: "button", class: "wizard__close", onclick: close, "×" }
                }
                div { class: "wizard__body",
                    if !snap.seed_track_ids.is_empty() {
                        p { class: "wizard__hint",
                            "{snap.seed_track_ids.len()} tracks from the queue will be saved."
                        }
                    }
                    label { class: "field",
                        span { class: "field__label", "Name" }
                        input {
                            r#type: "text",
                            class: "input",
                            placeholder: "Late night drives",
                            value: "{snap.name}",
                            oninput: on_name,
                            autofocus: true,
                        }
                    }
                    label { class: "field",
                        span { class: "field__label", "Description (optional)" }
                        input {
                            r#type: "text",
                            class: "input",
                            placeholder: "What's this playlist for?",
                            value: "{snap.description}",
                            oninput: on_desc,
                        }
                    }
                    if let Some(err) = &snap.error {
                        p { class: "wizard__error", "{err}" }
                    }
                }
                div { class: "wizard__footer",
                    button { r#type: "button", class: "btn btn--ghost", onclick: close, "Cancel" }
                    button {
                        r#type: "submit",
                        class: "btn btn--primary",
                        disabled: snap.name.trim().is_empty(),
                        "Create"
                    }
                }
            }
        }
    }
}
