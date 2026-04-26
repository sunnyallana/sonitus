//! Mobile mini-player — bottom strip with play/pause + next.

use crate::app::use_app_handle;
use crate::hooks::use_player::use_player;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::player::commands::PlayerCommand;

/// Mobile-collapsed mini player.
#[component]
pub fn MiniPlayer() -> Element {
    let player = use_player();
    let handle = use_app_handle();
    let state = player.read();
    let Some(track) = state.track else {
        return rsx! { div { class: "mini-player mini-player--empty" } };
    };
    let is_paused = state.is_paused;

    let h_pause = handle.clone();
    let on_play_pause = move |evt: MouseEvent| {
        evt.stop_propagation();
        evt.prevent_default();
        if let Some(h) = h_pause.clone() {
            let cmd = if is_paused { PlayerCommand::Resume } else { PlayerCommand::Pause };
            let _ = h.player.send(cmd);
        }
    };

    let h_next = handle.clone();
    let on_next = move |evt: MouseEvent| {
        evt.stop_propagation();
        evt.prevent_default();
        if let Some(h) = h_next.clone() { h.next(); }
    };

    rsx! {
        Link { to: Route::NowPlayingFull {}, class: "mini-player",
            span { class: "mini-player__title", "{track.title}" }
            button {
                class: "mini-player__btn",
                aria_label: if is_paused { "Play" } else { "Pause" },
                onclick: on_play_pause,
                if is_paused { "▶" } else { "⏸" }
            }
            button {
                class: "mini-player__btn",
                aria_label: "Next track",
                onclick: on_next,
                "⏭"
            }
        }
    }
}
