//! Fullscreen now-playing view.
//!
//! Large centered cover art, title + genre, full-width seekbar, transport
//! controls, volume. Reachable by clicking the now-playing bar.

use crate::components::library::cover_art::CoverArt;
use crate::components::player::controls::Controls;
use crate::components::player::seekbar::Seekbar;
use crate::components::player::volume_control::VolumeControl;
use crate::hooks::use_player::use_player;
use crate::routes::Route;
use dioxus::prelude::*;

/// Fullscreen "Now Playing" page.
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
            div { class: "now-playing-full__inner",
                div { class: "now-playing-full__cover",
                    if let Some(t) = state.track.as_ref() {
                        CoverArt {
                            album_id: t.album_id.clone(),
                            size_class: "cover-art--xl".to_string(),
                        }
                    } else {
                        div { class: "cover-art cover-art--xl",
                            div { class: "cover-art__placeholder", "♪" }
                        }
                    }
                }
                div { class: "now-playing-full__meta",
                    if let Some(t) = state.track.as_ref() {
                        h2 { class: "now-playing-full__title", "{t.title}" }
                        if let Some(g) = t.genre.clone() {
                            div { class: "now-playing-full__sub", "{g}" }
                        }
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
                div { class: "now-playing-full__volume",
                    VolumeControl {}
                }
            }
        }
    }
}
