//! Add-source wizard: kind picker → config → test → first scan.

use dioxus::prelude::*;

/// 4-step wizard for adding a new source.
#[component]
pub fn AddSourceWizard() -> Element {
    rsx! {
        dialog { class: "wizard",
            header { class: "wizard__header",
                h1 { "Add a source" }
            }
            ol { class: "wizard__steps",
                li { class: "wizard__step wizard__step--active", "1. Choose kind" }
                li { class: "wizard__step", "2. Configure" }
                li { class: "wizard__step", "3. Test connection" }
                li { class: "wizard__step", "4. First scan" }
            }
            div { class: "wizard__body",
                div { class: "wizard__kinds",
                    button { class: "kind-card", "Local folder" }
                    button { class: "kind-card", "Google Drive" }
                    button { class: "kind-card", "Amazon S3" }
                    button { class: "kind-card", "SMB / NAS" }
                    button { class: "kind-card", "HTTP server" }
                    button { class: "kind-card", "Dropbox" }
                    button { class: "kind-card", "OneDrive" }
                }
            }
            footer { class: "wizard__footer",
                button { class: "btn btn--ghost", "Cancel" }
                button { class: "btn btn--primary", "Next →" }
            }
        }
    }
}
