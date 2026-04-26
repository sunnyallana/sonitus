//! Dialog for creating a new manual playlist.
//!
//! Opened from the Playlists page header. Submits via
//! `queries::playlists::create_manual` and bumps `library_state.version`
//! so the playlists list refreshes.

use crate::app::use_app_handle;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// State signal for the new-playlist dialog. Provided at the Playlists
/// list level so the "+ New playlist" button + dialog can communicate.
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
        dioxus::prelude::spawn(async move {
            let res = queries::playlists::create_manual(
                handle.library.pool(),
                &name,
                desc_opt.as_deref(),
            )
            .await;
            match res {
                Ok(_) => {
                    state.set(NewPlaylistState::default());
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                }
                Err(e) => {
                    state.write().error = Some(format!("Failed: {e}"));
                }
            }
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
                    h1 { "New playlist" }
                    button { r#type: "button", class: "wizard__close", onclick: close, "×" }
                }
                div { class: "wizard__body",
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
