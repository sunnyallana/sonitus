//! Slide-in queue panel.
//!
//! State (open / closed) lives in a `Signal<QueuePanelState>` installed at
//! app scope so any component can toggle it. The panel is rendered by the
//! `AppShell` so it floats above all routed pages.
//!
//! Operations:
//! - **Per-row remove** sends `PlayerCommand::RemoveFromQueue { index }`.
//! - **Up/Down arrow buttons** send `PlayerCommand::MoveInQueue { from, to }`.
//! - **Clear** sends `ClearQueue` (keeps the currently-playing track).
//! - **Save as playlist** opens the new-playlist dialog with a "save the
//!   queue" carry-over flag — on submit, the dialog appends every queue
//!   track in order to the freshly-created playlist.

use crate::app::use_app_handle;
use crate::components::playlists::new_playlist_dialog::NewPlaylistState;
use crate::hooks::use_player::use_player;
use dioxus::prelude::*;
use sonitus_core::library::Track;
use sonitus_core::player::commands::PlayerCommand;

/// Controls visibility of the queue panel.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QueuePanelState {
    /// Whether the panel is currently visible.
    pub open: bool,
}

/// Install the panel-state signal at app scope. Called from `App`.
pub fn install_queue_panel_state() {
    use_context_provider(|| Signal::new(QueuePanelState::default()));
}

/// Slide-in queue panel — rendered once at the shell level.
#[component]
pub fn QueuePanel() -> Element {
    let mut state = use_context::<Signal<QueuePanelState>>();
    let mut new_playlist = use_context::<Signal<NewPlaylistState>>();
    let player = use_player();
    let handle = use_app_handle();

    if !state.read().open {
        return rsx! {};
    }

    let snap = player.read();
    let queue: Vec<Track> = snap.queue.clone();
    let current_id = snap.track.as_ref().map(|t| t.id.clone());

    let close = move |_| state.write().open = false;
    let stop_inside = move |evt: MouseEvent| evt.stop_propagation();

    let h_clear = handle.clone();
    let on_clear = move |_| {
        if let Some(h) = h_clear.clone() {
            let _ = h.player.send(PlayerCommand::ClearQueue);
        }
    };

    let queue_for_save = queue.clone();
    let on_save_as_playlist = move |_| {
        // Carry the queue's track IDs through the new-playlist dialog
        // so it can append them to the new playlist on submit.
        let track_ids: Vec<String> = queue_for_save.iter().map(|t| t.id.clone()).collect();
        new_playlist.set(NewPlaylistState {
            open: true,
            seed_track_ids: track_ids,
            ..NewPlaylistState::default()
        });
        state.write().open = false;
    };

    rsx! {
        div { class: "queue-backdrop", onclick: close,
            aside {
                class: "queue-panel",
                role: "complementary",
                aria_label: "Play queue",
                onclick: stop_inside,
                header { class: "queue-panel__header",
                    h2 { "Queue" }
                    button {
                        class: "queue-panel__close",
                        title: "Close queue",
                        onclick: close,
                        "×"
                    }
                }
                div { class: "queue-panel__actions",
                    button {
                        class: "btn btn--ghost btn--sm",
                        disabled: queue.is_empty(),
                        onclick: on_clear,
                        "Clear"
                    }
                    button {
                        class: "btn btn--ghost btn--sm",
                        disabled: queue.is_empty(),
                        onclick: on_save_as_playlist,
                        "Save as playlist"
                    }
                }
                if queue.is_empty() {
                    p { class: "queue-panel__empty",
                        "Queue is empty. Play a track or hit + on a row to enqueue."
                    }
                } else {
                    ol { class: "queue-panel__list",
                        for (i, track) in queue.iter().enumerate() {
                            QueueRow {
                                index: i,
                                queue_len: queue.len(),
                                track: track.clone(),
                                is_current: current_id.as_deref() == Some(track.id.as_str()),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QueueRow(index: usize, queue_len: usize, track: Track, is_current: bool) -> Element {
    let handle = use_app_handle();

    let h_play = handle.clone();
    let track_id_play = track.id.clone();
    let on_play = move |_| {
        if let Some(h) = h_play.clone() {
            h.play(track_id_play.clone());
        }
    };

    let h_remove = handle.clone();
    let on_remove = move |evt: MouseEvent| {
        evt.stop_propagation();
        if let Some(h) = h_remove.clone() {
            let _ = h.player.send(PlayerCommand::RemoveFromQueue { index });
        }
    };

    let h_up = handle.clone();
    let on_up = move |evt: MouseEvent| {
        evt.stop_propagation();
        if index == 0 { return; }
        if let Some(h) = h_up.clone() {
            let _ = h.player.send(PlayerCommand::MoveInQueue { from: index, to: index - 1 });
        }
    };

    let h_down = handle.clone();
    let on_down = move |evt: MouseEvent| {
        evt.stop_propagation();
        if index + 1 >= queue_len { return; }
        if let Some(h) = h_down.clone() {
            let _ = h.player.send(PlayerCommand::MoveInQueue { from: index, to: index + 1 });
        }
    };

    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    let row_class = if is_current {
        "queue-panel__row queue-panel__row--current"
    } else {
        "queue-panel__row"
    };
    let can_remove = !is_current;
    let can_up = index > 0;
    let can_down = index + 1 < queue_len;

    rsx! {
        li { class: row_class, ondoubleclick: on_play,
            span { class: "queue-panel__pos", "{index + 1}" }
            div { class: "queue-panel__meta",
                span { class: "queue-panel__title", "{track.title}" }
                span { class: "queue-panel__sub",
                    "{track.duration_ms.map(|_| String::new()).unwrap_or_default()}{dur}"
                }
            }
            div { class: "queue-panel__row-actions",
                button {
                    class: "btn--icon",
                    title: "Move up",
                    aria_label: "Move up",
                    disabled: !can_up,
                    onclick: on_up,
                    "↑"
                }
                button {
                    class: "btn--icon",
                    title: "Move down",
                    aria_label: "Move down",
                    disabled: !can_down,
                    onclick: on_down,
                    "↓"
                }
                button {
                    class: "btn--icon",
                    title: if can_remove { "Remove from queue" } else { "Currently playing" },
                    aria_label: "Remove from queue",
                    disabled: !can_remove,
                    onclick: on_remove,
                    "×"
                }
            }
        }
    }
}

fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
