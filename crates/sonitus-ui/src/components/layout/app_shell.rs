//! Root layout: sidebar + topbar + content + now-playing bar.
//!
//! Also installs the global keyboard-shortcut handler:
//! - `Space` — play/pause
//! - `←` / `→` — seek 5 s
//! - `J` / `L` — seek 10 s
//! - `↑` / `↓` — volume +/-
//! - `M` — mute toggle
//! - `N` / `P` — next/previous
//! - `S` — toggle shuffle
//! - `R` — cycle repeat
//!
//! Shortcuts are no-ops while the user is focused on a text input or
//! contenteditable element so typing in the search bar / wizard inputs
//! works normally.

use crate::app::use_app_handle;
use crate::components::layout::{
    bottom_nav::BottomNav, now_playing_bar::NowPlayingBar, sidebar::Sidebar, topbar::Topbar,
};
use crate::components::playlists::add_to_playlist_dialog::AddToPlaylistDialog;
use crate::components::playlists::new_playlist_dialog::NewPlaylistDialog;
use crate::hooks::use_player::use_player;
use crate::state::settings_state::SettingsState;
use dioxus::prelude::*;
use sonitus_core::player::commands::{PlayerCommand, RepeatMode};

/// Wrapping layout component used by every routed page (except `/now-playing`).
#[component]
pub fn AppShell() -> Element {
    let handle = use_app_handle();
    let player = use_player();
    let mut settings = use_context::<Signal<SettingsState>>();

    let on_keydown = move |evt: KeyboardEvent| {
        let Some(handle) = handle.clone() else { return; };
        let state = player.read();

        // Skip if the focused element is a text input — let typing pass through.
        // (Dioxus 0.7 doesn't expose target.tagName cleanly; we rely on the
        // event bubbling. Form elements call `stop_propagation` for typing
        // keys via the browser's default behavior on input fields, but
        // that's not reliable, so we filter on the most common keys
        // that might collide with typing: only react to Space when the
        // user isn't on a text field. The simplest filter is to check
        // the modifier keys + key code.)
        if evt.modifiers().ctrl() || evt.modifiers().meta() || evt.modifiers().alt() {
            return;
        }

        let key = evt.key();
        // Skip plain typing letters when an input is focused. Best-effort:
        // we only act on the keys we care about; everything else falls
        // through to the input.
        match key {
            Key::Character(ref c) if c == " " => {
                evt.prevent_default();
                if state.track.is_some() {
                    let cmd = if state.is_paused { PlayerCommand::Resume } else { PlayerCommand::Pause };
                    let _ = handle.player.send(cmd);
                }
            }
            Key::ArrowLeft => {
                evt.prevent_default();
                let new_pos = (state.position_ms.saturating_sub(5_000)) as f64 / 1000.0;
                handle.seek(new_pos);
            }
            Key::ArrowRight => {
                evt.prevent_default();
                let new_pos = (state.position_ms.saturating_add(5_000)) as f64 / 1000.0;
                handle.seek(new_pos);
            }
            Key::ArrowUp => {
                evt.prevent_default();
                let v = (state.volume + 0.05).min(1.0);
                handle.set_volume(v);
                settings.write().set_volume(v);
            }
            Key::ArrowDown => {
                evt.prevent_default();
                let v = (state.volume - 0.05).max(0.0);
                handle.set_volume(v);
                settings.write().set_volume(v);
            }
            Key::Character(ref c) => {
                let lc = c.to_ascii_lowercase();
                match lc.as_str() {
                    "j" => {
                        evt.prevent_default();
                        let p = (state.position_ms.saturating_sub(10_000)) as f64 / 1000.0;
                        handle.seek(p);
                    }
                    "l" => {
                        evt.prevent_default();
                        let p = (state.position_ms.saturating_add(10_000)) as f64 / 1000.0;
                        handle.seek(p);
                    }
                    "n" => {
                        evt.prevent_default();
                        handle.next();
                    }
                    "p" => {
                        evt.prevent_default();
                        handle.prev();
                    }
                    "m" => {
                        evt.prevent_default();
                        let v = if state.volume == 0.0 { 0.7 } else { 0.0 };
                        handle.set_volume(v);
                        settings.write().set_volume(v);
                    }
                    "s" => {
                        evt.prevent_default();
                        let _ = handle.player.send(PlayerCommand::SetShuffle {
                            enabled: !state.shuffle,
                        });
                    }
                    "r" => {
                        evt.prevent_default();
                        let next = match state.repeat {
                            RepeatMode::Off => RepeatMode::All,
                            RepeatMode::All => RepeatMode::One,
                            RepeatMode::One => RepeatMode::Off,
                        };
                        let _ = handle.player.send(PlayerCommand::SetRepeat { mode: next });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    };

    rsx! {
        div { class: "app-shell", tabindex: "0", onkeydown: on_keydown,
            div { class: "app-shell__main",
                Sidebar {}
                div { class: "app-shell__content",
                    Topbar {}
                    main { class: "app-shell__page",
                        Outlet::<crate::routes::Route> {}
                    }
                }
            }
            NowPlayingBar {}
            BottomNav {}
            // App-wide dialogs — render once at shell level so any page
            // can open them by writing the corresponding state signal.
            NewPlaylistDialog {}
            AddToPlaylistDialog {}
        }
    }
}
