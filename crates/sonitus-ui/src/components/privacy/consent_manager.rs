//! Consent manager — toggles for opt-in features with disclosure text.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::privacy::Feature;

/// Consent manager UI.
#[component]
pub fn ConsentManager() -> Element {
    rsx! {
        section { class: "consent-manager",
            h1 { "Opt-in features" }
            p { class: "consent-manager__intro",
                "These features make outbound calls beyond what you directly initiate. They are off by default."
            }
            FeatureToggle { feature: Feature::MetadataLookups }
            FeatureToggle { feature: Feature::AcoustidFingerprinting }
        }
    }
}

#[component]
fn FeatureToggle(feature: Feature) -> Element {
    let handle = use_app_handle();
    let mut tick = use_signal(|| 0u64);
    // Read the current state. We bump `tick` to force re-render after a write.
    let enabled = handle
        .as_ref()
        .map(|h| h.consent.is_enabled(feature))
        .unwrap_or(false);
    let _ = tick.read(); // tie re-render to tick

    let name = feature.display_name();
    let desc = feature.disclosure();
    let what = feature.what_is_sent();
    let h = handle.clone();
    let on_toggle = move |_| {
        if let Some(handle) = h.clone() {
            let new_state = !handle.consent.is_enabled(feature);
            let _ = handle.consent.set(feature, new_state);
            tick.set(tick.peek().wrapping_add(1));
        }
    };

    rsx! {
        div { class: "consent-toggle",
            div { class: "consent-toggle__header",
                h3 { "{name}" }
                label { class: "switch",
                    input { r#type: "checkbox", checked: enabled, onchange: on_toggle }
                    span { class: "switch__slider" }
                }
            }
            p { class: "consent-toggle__desc", "{desc}" }
            p { class: "consent-toggle__what",
                strong { "What is sent: " }
                "{what}"
            }
        }
    }
}
