//! List of connected sources.

use crate::app::use_app_handle;
use crate::components::sources::add_source_wizard::{AddSourceWizard, WizardState};
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Sources page — list each source, with rescan/disable buttons + add wizard.
#[component]
pub fn SourcesList() -> Element {
    // Provide the wizard state at this scope so this component's button
    // and the wizard component share it.
    let mut wizard = use_context_provider(|| Signal::new(WizardState::default()));

    let handle = use_app_handle();
    let wizard_open = wizard.read().open;
    let wizard_done = wizard.read().done;
    // Re-fetch sources whenever the wizard transitions from open→closed
    // (so a freshly-scanned source appears immediately).
    let sources = use_resource(move || {
        let h = handle.clone();
        // Read these so the resource re-runs when they change.
        let _ = wizard_open;
        let _ = wizard_done;
        async move {
            let h = h?;
            queries::sources::list_all(h.library.pool()).await.ok()
        }
    });

    rsx! {
        section { class: "sources-list",
            header { class: "sources-list__header",
                h1 { "Sources" }
                button {
                    class: "btn btn--primary",
                    onclick: move |_| {
                        let mut w = wizard.write();
                        *w = WizardState { open: true, ..WizardState::default() };
                    },
                    "+ Add source"
                }
            }
            ul { class: "sources-list__items",
                match &*sources.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for src in rows.iter() {
                            li { class: "source-row", key: "{src.id}",
                                div { class: "source-row__name",
                                    Link { to: Route::SourceDetail { id: src.id.clone() }, "{src.name}" }
                                }
                                div { class: "source-row__kind", "{src.kind}" }
                                div { class: "source-row__count", "{src.track_count} tracks" }
                                div { class: "source-row__state", "{src.scan_state}" }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "sources-list__empty",
                            "No sources yet. Add one to start indexing music."
                        }
                    },
                    None => rsx! {
                        p { class: "sources-list__empty", "Loading..." }
                    },
                }
            }

            AddSourceWizard {}
        }
    }
}
