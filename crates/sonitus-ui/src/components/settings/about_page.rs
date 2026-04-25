//! About page.

use crate::hooks::use_keyboard::shortcut_list;
use dioxus::prelude::*;

/// About + licenses + shortcuts.
#[component]
pub fn AboutPage() -> Element {
    rsx! {
        section { class: "settings-page about-page",
            h1 { "About Sonitus" }
            p { class: "about-page__version", "Version {sonitus_core::VERSION}" }
            p { "Sonitus is a local-first, encrypted music streaming and library application." }

            section { class: "about-page__section",
                h2 { "Privacy guarantees" }
                ul {
                    li { "Local-first: no server, no cloud sync unless you set it up." }
                    li { "Encrypted: SQLite vault sealed with XChaCha20-Poly1305 + Argon2id." }
                    li { "Zero telemetry: no analytics, no crash reporting." }
                    li { "Credential isolation: OAuth tokens encrypted, zeroed on drop." }
                    li { "Auditable: every outbound request is logged." }
                }
            }

            section { class: "about-page__section",
                h2 { "Keyboard shortcuts" }
                table { class: "shortcut-table",
                    thead {
                        tr { th { "Keys" } th { "Action" } }
                    }
                    tbody {
                        for (keys, action) in shortcut_list() {
                            tr {
                                td { class: "shortcut-table__keys", "{keys}" }
                                td { "{action}" }
                            }
                        }
                    }
                }
            }

            section { class: "about-page__section",
                h2 { "Open source" }
                p { "Sonitus is MIT-licensed. Audit the code on GitHub." }
            }
        }
    }
}
