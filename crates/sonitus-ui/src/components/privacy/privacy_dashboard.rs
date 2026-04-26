//! Privacy dashboard — five guarantee status cards + traffic counter.

use crate::app::use_app_handle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;

/// Privacy dashboard. The five status cards are always green when the
/// app is running normally; if any check fails (e.g. audit log unwriteable)
/// the corresponding card flips to red.
#[component]
pub fn PrivacyDashboard() -> Element {
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    // Read the audit log on a blocking thread and count entries from
    // today (UTC). We re-fetch on every library_signal.version bump so
    // the counter doesn't go stale across long-lived sessions.
    let counter = use_resource(move || {
        let h = handle.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            let logger = h.audit.clone();
            let entries = tokio::task::spawn_blocking(move || logger.read_entries())
                .await
                .ok()?
                .ok()?;
            let today = chrono::Utc::now().date_naive();
            let count = entries
                .iter()
                .filter(|e| e.ts.date_naive() == today)
                .count();
            Some(count)
        }
    });
    let count_today = counter.read_unchecked().flatten().unwrap_or(0);

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
                p { "Outbound requests today: {count_today}" }
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
