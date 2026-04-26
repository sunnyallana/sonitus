//! Appearance settings — wired to SettingsState (persists to config.toml).

use crate::state::settings_state::SettingsState;
use dioxus::prelude::*;
use sonitus_core::config::{LibraryView, Theme};

/// Appearance settings.
#[component]
pub fn AppearanceSettings() -> Element {
    let mut settings = use_context::<Signal<SettingsState>>();
    let snap = settings.read().clone();
    let theme = snap.config.theme;
    let theme_value = match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
        Theme::System => "system",
    };
    let accent = snap.config.accent_color.clone();
    let library_view = snap.config.library_view;

    let on_library_view = move |evt: FormEvent| {
        let new = match evt.value().as_str() {
            "list" => LibraryView::List,
            _ => LibraryView::Grid,
        };
        settings.write().set_library_view(new);
    };

    let on_theme = move |evt: FormEvent| {
        let new = match evt.value().as_str() {
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            _ => Theme::System,
        };
        settings.write().set_theme(new);
    };

    let on_accent = move |evt: FormEvent| {
        let v = evt.value();
        let mut s = settings.write();
        s.config.accent_color = v;
        // Re-apply the existing theme to trigger persistence; .set_theme
        // is what owns the write-through-to-disk path.
        let cur = s.config.theme;
        s.set_theme(cur);
    };

    rsx! {
        section { class: "settings-page",
            h1 { "Appearance" }
            label { class: "field",
                span { class: "field__label", "Theme" }
                select { class: "select", value: "{theme_value}", onchange: on_theme,
                    option { value: "system", selected: matches!(theme, Theme::System), "Match system" }
                    option { value: "dark",   selected: matches!(theme, Theme::Dark),   "Dark" }
                    option { value: "light",  selected: matches!(theme, Theme::Light),  "Light" }
                }
            }
            label { class: "field",
                span { class: "field__label", "Accent color" }
                input { r#type: "color", value: "{accent}", class: "color-picker", onchange: on_accent }
            }
            label { class: "field",
                span { class: "field__label", "Library default view" }
                select { class: "select", onchange: on_library_view,
                    option { value: "grid", selected: matches!(library_view, LibraryView::Grid), "Grid" }
                    option { value: "list", selected: matches!(library_view, LibraryView::List), "List" }
                }
            }
        }
    }
}
