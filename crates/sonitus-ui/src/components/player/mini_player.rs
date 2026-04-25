//! Mobile mini-player — bottom strip with play/pause + next.

use crate::hooks::use_player::use_player;
use crate::routes::Route;
use dioxus::prelude::*;

/// Mobile-collapsed mini player.
#[component]
pub fn MiniPlayer() -> Element {
    let player = use_player();
    let state = player.read();
    let Some(track) = state.track else {
        return rsx! { div { class: "mini-player mini-player--empty" } };
    };

    rsx! {
        Link { to: Route::NowPlayingFull {}, class: "mini-player",
            span { class: "mini-player__title", "{track.title}" }
            button { class: "mini-player__btn",
                if state.is_paused { "▶" } else { "⏸" }
            }
            button { class: "mini-player__btn", "⏭" }
        }
    }
}
