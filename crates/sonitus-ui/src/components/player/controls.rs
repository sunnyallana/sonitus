//! ⏮ ⏯ ⏭ + shuffle + repeat controls.

use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Transport controls.
#[component]
pub fn Controls() -> Element {
    let player = use_player();
    let state = player.read();

    rsx! {
        div { class: "controls", role: "group", aria_label: "Playback controls",
            button { class: "controls__btn controls__btn--shuffle",
                aria_pressed: "{state.shuffle}",
                title: "Toggle shuffle",
                "⇄"
            }
            button { class: "controls__btn controls__btn--prev",
                title: "Previous track",
                "⏮"
            }
            button { class: "controls__btn controls__btn--play",
                title: if state.is_paused { "Play" } else { "Pause" },
                if state.is_paused { "▶" } else { "⏸" }
            }
            button { class: "controls__btn controls__btn--next",
                title: "Next track",
                "⏭"
            }
            button { class: "controls__btn controls__btn--repeat",
                title: "Cycle repeat mode",
                "↻"
            }
        }
    }
}
