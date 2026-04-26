//! Audio settings — wired to SettingsState (persists to config.toml) and
//! the player engine.
//!
//! Output device, ReplayGain mode, crossfade, gapless, and buffer size
//! all write through to `AppConfig` and (where the engine cares) emit a
//! corresponding `PlayerCommand` so changes apply to the live audio
//! pipeline without restart.

use crate::app::use_app_handle;
use crate::state::settings_state::SettingsState;
use dioxus::prelude::*;
use sonitus_core::config::{BufferSize, ReplayGainMode};
use sonitus_core::player::commands::{PlayerCommand, ReplayGainCommand};

#[cfg(not(target_arch = "wasm32"))]
use sonitus_core::player::output_native::NativeOutput;

/// Audio settings page.
#[component]
pub fn AudioSettings() -> Element {
    let mut settings = use_context::<Signal<SettingsState>>();
    let handle = use_app_handle();

    let snap = settings.read().clone();
    let crossfade_secs = snap.config.crossfade_secs;
    let gapless = snap.config.gapless_enabled;
    let rg = snap.config.replay_gain_mode;
    let buf = snap.config.buffer_size;
    let current_device = snap.config.output_device.clone().unwrap_or_default();

    // Enumerate output devices once on mount. cpal listing is a sync
    // call; cheap, so we do it on render and memoize via use_memo.
    let devices = use_memo(move || {
        #[cfg(not(target_arch = "wasm32"))]
        {
            NativeOutput::list_devices().unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Vec::<String>::new()
        }
    });

    let h_device = handle.clone();
    let on_device = move |evt: FormEvent| {
        let v = evt.value();
        let name = if v.is_empty() { None } else { Some(v.clone()) };
        settings.write().set_output_device(name.clone());
        if let Some(h) = h_device.clone() {
            let _ = h.player.send(PlayerCommand::SetOutputDevice { name });
        }
    };

    let h_rg = handle.clone();
    let on_replay_gain = move |evt: FormEvent| {
        let mode = match evt.value().as_str() {
            "track" => ReplayGainMode::Track,
            "album" => ReplayGainMode::Album,
            _ => ReplayGainMode::Off,
        };
        settings.write().set_replay_gain(mode);
        if let Some(h) = h_rg.clone() {
            let cmd = match mode {
                ReplayGainMode::Off => ReplayGainCommand::Off,
                ReplayGainMode::Track => ReplayGainCommand::Track,
                ReplayGainMode::Album => ReplayGainCommand::Album,
            };
            let _ = h.player.send(PlayerCommand::SetReplayGain { mode: cmd });
        }
    };

    let on_crossfade = move |evt: FormEvent| {
        let v: f32 = evt.value().parse().unwrap_or(0.0);
        settings.write().set_crossfade_secs(v);
    };

    let on_gapless = move |evt: FormEvent| {
        let v = evt.value();
        // Dioxus checkbox events surface "true"/"false" strings.
        let on = v == "true" || v == "on" || v == "1";
        settings.write().set_gapless(on);
    };

    let on_buffer = move |evt: FormEvent| {
        let s = match evt.value().as_str() {
            "small" => BufferSize::Small,
            "large" => BufferSize::Large,
            _ => BufferSize::Medium,
        };
        settings.write().set_buffer_size(s);
    };

    rsx! {
        section { class: "settings-page",
            h1 { "Audio" }
            label { class: "field",
                span { class: "field__label", "Output device" }
                select { class: "select", value: "{current_device}", onchange: on_device,
                    option { value: "", "System default" }
                    for name in devices.read().iter() {
                        option { value: "{name}", selected: name == &current_device, "{name}" }
                    }
                }
                span { class: "field__hint",
                    "Switching device restarts the audio stream on the next track."
                }
            }
            label { class: "field",
                span { class: "field__label", "ReplayGain" }
                select { class: "select", onchange: on_replay_gain,
                    option { value: "off",   selected: matches!(rg, ReplayGainMode::Off),   "Off" }
                    option { value: "track", selected: matches!(rg, ReplayGainMode::Track), "Track" }
                    option { value: "album", selected: matches!(rg, ReplayGainMode::Album), "Album" }
                }
            }
            label { class: "field",
                span { class: "field__label", "Crossfade ({crossfade_secs as u32}s)" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "12",
                    step: "1",
                    value: "{crossfade_secs as u32}",
                    class: "range",
                    oninput: on_crossfade,
                }
            }
            label { class: "field field--inline",
                input {
                    r#type: "checkbox",
                    checked: gapless,
                    oninput: on_gapless,
                }
                span { "Gapless playback" }
            }
            label { class: "field",
                span { class: "field__label", "Buffer size" }
                select { class: "select", onchange: on_buffer,
                    option { value: "small",  selected: matches!(buf, BufferSize::Small),  "Small (lowest latency)" }
                    option { value: "medium", selected: matches!(buf, BufferSize::Medium), "Medium" }
                    option { value: "large",  selected: matches!(buf, BufferSize::Large),  "Large (smoothest)" }
                }
                span { class: "field__hint",
                    "Buffer size applies on the next track."
                }
            }
        }
    }
}
