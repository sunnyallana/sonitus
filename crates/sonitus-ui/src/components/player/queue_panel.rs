//! Slide-in queue panel.

use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Queue panel — drag to reorder, clear, save-as-playlist.
#[component]
pub fn QueuePanel() -> Element {
    let player = use_player();
    let queue = player.read().queue;

    rsx! {
        aside { class: "queue-panel", aria_label: "Play queue",
            div { class: "queue-panel__header",
                h2 { "Queue" }
                button { class: "queue-panel__clear", "Clear" }
                button { class: "queue-panel__save", "Save as playlist" }
            }
            ul { class: "queue-panel__list",
                for (i, track) in queue.iter().enumerate() {
                    li { class: "queue-panel__item", key: "{track.id}",
                        span { class: "queue-panel__pos", "{i + 1}" }
                        span { class: "queue-panel__title", "{track.title}" }
                    }
                }
            }
            if queue.is_empty() {
                div { class: "queue-panel__empty", "Queue is empty." }
            }
        }
    }
}
