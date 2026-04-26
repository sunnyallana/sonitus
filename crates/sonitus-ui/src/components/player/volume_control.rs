//! Volume slider + mute button.
//!
//! Volume is the source of truth in the engine; this component echoes
//! changes to `SettingsState` so they persist across launches.

use crate::app::use_app_handle;
use crate::hooks::use_player::use_player;
use crate::state::settings_state::SettingsState;
use dioxus::prelude::*;

/// Volume control widget.
#[component]
pub fn VolumeControl() -> Element {
    let player = use_player();
    let vol = player.read().volume;
    let pct = (vol * 100.0) as u32;
    let handle = use_app_handle();
    let mut settings = use_context::<Signal<SettingsState>>();

    let h = handle.clone();
    let on_input = move |evt: FormEvent| {
        let Some(handle) = h.clone() else { return; };
        let Ok(p) = evt.value().parse::<f32>() else { return; };
        let amp = (p / 100.0).clamp(0.0, 1.0);
        handle.set_volume(amp);
        settings.write().set_volume(amp);
    };

    let h = handle.clone();
    let mut last_volume_before_mute = use_signal(|| 0.7f32);
    let on_mute_toggle = move |_| {
        let Some(handle) = h.clone() else { return; };
        if vol == 0.0 {
            let v = *last_volume_before_mute.read();
            handle.set_volume(v);
            settings.write().set_volume(v);
        } else {
            last_volume_before_mute.set(vol);
            handle.set_volume(0.0);
            settings.write().set_volume(0.0);
        }
    };

    rsx! {
        div { class: "volume",
            button { class: "volume__mute",
                onclick: on_mute_toggle,
                title: if vol == 0.0 { "Unmute" } else { "Mute" },
                if vol == 0.0 { "🔇" } else if vol < 0.4 { "🔈" } else if vol < 0.7 { "🔉" } else { "🔊" }
            }
            input { class: "volume__slider",
                r#type: "range",
                min: "0",
                max: "100",
                value: "{pct}",
                aria_label: "Volume",
                oninput: on_input,
            }
        }
    }
}
