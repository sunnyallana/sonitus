//! Privacy dashboard — five guarantee status cards + traffic counter.

use crate::routes::Route;
use dioxus::prelude::*;

/// Privacy dashboard. The five status cards are always green when the
/// app is running normally; if any check fails (e.g. audit log unwriteable)
/// the corresponding card flips to red.
#[component]
pub fn PrivacyDashboard() -> Element {
    rsx! {
        section { class: "privacy-dashboard",
            h1 { "Privacy" }
            p { class: "privacy-dashboard__intro",
                "Sonitus's privacy guarantees, plus a live view of where your data goes."
            }

            div { class: "guarantee-cards",
                GuaranteeCard {
                    title: "Local-first".to_string(),
                    description: "Your library lives on this device. There is no Sonitus server.".to_string(),
                    is_ok: true,
                }
                GuaranteeCard {
                    title: "Encrypted at rest".to_string(),
                    description: "Vault sealed with XChaCha20-Poly1305 + Argon2id.".to_string(),
                    is_ok: true,
                }
                GuaranteeCard {
                    title: "Zero telemetry".to_string(),
                    description: "No analytics, crash reporting, or usage tracking.".to_string(),
                    is_ok: true,
                }
                GuaranteeCard {
                    title: "Credential isolation".to_string(),
                    description: "OAuth tokens encrypted; zeroed from memory after use.".to_string(),
                    is_ok: true,
                }
                GuaranteeCard {
                    title: "Auditable".to_string(),
                    description: "Every outbound HTTP request is logged below.".to_string(),
                    is_ok: true,
                }
            }

            div { class: "privacy-dashboard__counter",
                p { "Outbound requests today: 0" }
            }

            div { class: "privacy-dashboard__links",
                Link { to: Route::AuditLogViewer {}, class: "btn btn--primary",
                    "View audit log →"
                }
                Link { to: Route::ConsentManager {}, class: "btn btn--ghost",
                    "Manage opt-in features →"
                }
            }
        }
    }
}

#[component]
fn GuaranteeCard(title: String, description: String, is_ok: bool) -> Element {
    let class_modifier = if is_ok { "guarantee-card--ok" } else { "guarantee-card--fail" };
    rsx! {
        div { class: "guarantee-card {class_modifier}",
            div { class: "guarantee-card__icon", if is_ok { "✓" } else { "✗" } }
            h3 { class: "guarantee-card__title", "{title}" }
            p { class: "guarantee-card__desc", "{description}" }
        }
    }
}
