//! Sticky now-playing bar at the bottom of every page.

use crate::components::player::controls::Controls;
use crate::components::player::seekbar::Seekbar;
use crate::components::player::volume_control::VolumeControl;
use crate::hooks::use_player::use_player;
use crate::routes::Route;
use dioxus::prelude::*;

/// Now-playing bar — appears on every screen except `/now-playing` (mobile).
#[component]
pub fn NowPlayingBar() -> Element {
    let player = use_player();
    let state = player.read();
    let Some(track) = state.track else {
        return rsx! { div { class: "now-playing-bar now-playing-bar--empty" } };
    };

    rsx! {
        div { class: "now-playing-bar", role: "region", aria_label: "Now playing",
            div { class: "now-playing-bar__track",
                div { class: "now-playing-bar__art",
                    Link { to: Route::NowPlayingFull {},
                        // Cover art stub: real cover would render an <img>
                        // sourced from the track's album.cover_art_blob.
                    }
                }
                div { class: "now-playing-bar__meta",
                    div { class: "now-playing-bar__title", "{track.title}" }
                    div { class: "now-playing-bar__sub",
                        if let Some(_aid) = track.artist_id { "—" } else { "" }
                    }
                }
            }
            div { class: "now-playing-bar__controls",
                Controls {}
                Seekbar {}
            }
            div { class: "now-playing-bar__right",
                VolumeControl {}
            }
        }
    }
}
