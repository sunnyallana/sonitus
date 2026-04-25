//! Settings landing — sub-navigation.

use crate::routes::Route;
use dioxus::prelude::*;

/// Settings home — sub-nav of areas.
#[component]
pub fn SettingsRoot() -> Element {
    rsx! {
        section { class: "settings-root",
            h1 { "Settings" }
            nav { class: "settings-nav",
                Link { to: Route::AudioSettings {}, class: "settings-nav__item",
                    h2 { "Audio" }
                    p { "Output device, ReplayGain, gapless, crossfade" }
                }
                Link { to: Route::AppearanceSettings {}, class: "settings-nav__item",
                    h2 { "Appearance" }
                    p { "Theme, accent color, font size" }
                }
                Link { to: Route::StorageSettings {}, class: "settings-nav__item",
                    h2 { "Storage" }
                    p { "Cache size, downloads location" }
                }
                Link { to: Route::PrivacyDashboard {}, class: "settings-nav__item",
                    h2 { "Privacy" }
                    p { "Audit log, opt-in features, data export" }
                }
                Link { to: Route::AboutPage {}, class: "settings-nav__item",
                    h2 { "About" }
                    p { "Version, licenses, GitHub" }
                }
            }
        }
    }
}
