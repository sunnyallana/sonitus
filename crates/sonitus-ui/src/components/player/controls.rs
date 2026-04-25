//! ⏮ ⏯ ⏭ + shuffle + repeat controls.

use crate::app::use_app_handle;
use crate::hooks::use_player::use_player;
use dioxus::prelude::*;
use sonitus_core::player::commands::{PlayerCommand, RepeatMode};

/// Transport controls.
#[component]
pub fn Controls() -> Element {
    let player = use_player();
    let state = player.read();
    let handle = use_app_handle();

    let h = handle.clone();
    let toggle_play = move |_| {
        if let Some(handle) = h.clone() {
            let cmd = if state.is_paused { PlayerCommand::Resume } else { PlayerCommand::Pause };
            let _ = handle.player.send(cmd);
        }
    };

    let h = handle.clone();
    let prev = move |_| {
        if let Some(handle) = h.clone() { handle.prev(); }
    };

    let h = handle.clone();
    let next = move |_| {
        if let Some(handle) = h.clone() { handle.next(); }
    };

    let h = handle.clone();
    let cur_shuffle = state.shuffle;
    let toggle_shuffle = move |_| {
        if let Some(handle) = h.clone() {
            let _ = handle.player.send(PlayerCommand::SetShuffle { enabled: !cur_shuffle });
        }
    };

    let h = handle.clone();
    let cur_repeat = state.repeat;
    let cycle_repeat = move |_| {
        if let Some(handle) = h.clone() {
            let next_mode = match cur_repeat {
                RepeatMode::Off => RepeatMode::All,
                RepeatMode::All => RepeatMode::One,
                RepeatMode::One => RepeatMode::Off,
            };
            let _ = handle.player.send(PlayerCommand::SetRepeat { mode: next_mode });
        }
    };

    rsx! {
        div { class: "controls", role: "group", aria_label: "Playback controls",
            button { class: "controls__btn controls__btn--shuffle",
                aria_pressed: "{state.shuffle}",
                onclick: toggle_shuffle,
                title: "Toggle shuffle",
                "⇄"
            }
            button { class: "controls__btn controls__btn--prev",
                onclick: prev,
                title: "Previous track",
                "⏮"
            }
            button { class: "controls__btn controls__btn--play",
                onclick: toggle_play,
                title: if state.is_paused { "Play" } else { "Pause" },
                if state.is_paused { "▶" } else { "⏸" }
            }
            button { class: "controls__btn controls__btn--next",
                onclick: next,
                title: "Next track",
                "⏭"
            }
            button { class: "controls__btn controls__btn--repeat",
                onclick: cycle_repeat,
                title: "Cycle repeat mode",
                {match state.repeat { RepeatMode::Off => "↻", RepeatMode::All => "🔁", RepeatMode::One => "🔂" }}
            }
        }
    }
}
