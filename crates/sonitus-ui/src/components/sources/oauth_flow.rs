//! OAuth callback handling for Google Drive / Dropbox / OneDrive.

use dioxus::prelude::*;

/// OAuth flow status component.
#[component]
pub fn OAuthFlow() -> Element {
    rsx! {
        section { class: "oauth-flow",
            h1 { "Authorize Sonitus" }
            p { "We're opening your browser to complete authentication. Sonitus only requests read-only access to your music." }
            div { class: "oauth-flow__pending",
                p { "Waiting for the browser to redirect back..." }
            }
        }
    }
}
