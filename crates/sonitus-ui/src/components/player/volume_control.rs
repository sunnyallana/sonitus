//! Volume slider + mute button.

use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Volume control widget.
#[component]
pub fn VolumeControl() -> Element {
    let player = use_player();
    let vol = player.read().volume;
    let pct = (vol * 100.0) as u32;

    rsx! {
        div { class: "volume",
            button { class: "volume__mute",
                title: if vol == 0.0 { "Unmute" } else { "Mute" },
                if vol == 0.0 { "🔇" } else if vol < 0.4 { "🔈" } else if vol < 0.7 { "🔉" } else { "🔊" }
            }
            input { class: "volume__slider",
                r#type: "range",
                min: "0",
                max: "100",
                value: "{pct}",
                aria_label: "Volume",
            }
        }
    }
}
