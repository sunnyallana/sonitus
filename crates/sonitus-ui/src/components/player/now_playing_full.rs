//! Mobile fullscreen now-playing view.

use crate::components::player::controls::Controls;
use crate::components::player::seekbar::Seekbar;
use crate::hooks::use_player::use_player;
use crate::routes::Route;
use dioxus::prelude::*;

/// Fullscreen "Now Playing" page (mobile primarily).
#[component]
pub fn NowPlayingFull() -> Element {
    let player = use_player();
    let state = player.read();

    rsx! {
        div { class: "now-playing-full",
            div { class: "now-playing-full__header",
                Link { to: Route::LibraryHome {}, class: "now-playing-full__back",
                    "↓ Close"
                }
            }
            div { class: "now-playing-full__art" }
            div { class: "now-playing-full__meta",
                if let Some(t) = &state.track {
                    h2 { class: "now-playing-full__title", "{t.title}" }
                } else {
                    h2 { class: "now-playing-full__title now-playing-full__title--empty",
                        "Nothing playing"
                    }
                }
            }
            div { class: "now-playing-full__seek",
                Seekbar {}
            }
            div { class: "now-playing-full__controls",
                Controls {}
            }
        }
    }
}
