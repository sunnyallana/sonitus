//! Consent manager — toggles for opt-in features with disclosure text.

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
    let name = feature.display_name();
    let desc = feature.disclosure();
    let what = feature.what_is_sent();

    rsx! {
        div { class: "consent-toggle",
            div { class: "consent-toggle__header",
                h3 { "{name}" }
                label { class: "switch",
                    input { r#type: "checkbox" }
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
