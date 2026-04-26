//! Playlist detail — header + track list + edit button.
//!
//! Resolves tracks differently based on playlist type:
//! - **Manual**: reads `playlist_tracks` rows ordered by position.
//! - **Smart**: parses `smart_rules` JSON and runs the rule engine
//!   (`playlist::smart::evaluate`) so the displayed tracks always reflect
//!   current library state — never a stale snapshot.

use crate::app::use_app_handle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::{Playlist, Track, queries};
use sonitus_core::playlist::smart::{self, SmartRules};

/// Resolved view: either a successfully-loaded playlist with its track
/// list, or the per-step error encountered while resolving.
#[derive(Clone, PartialEq)]
struct DetailView {
    playlist: Playlist,
    tracks: Vec<Track>,
}

/// Playlist detail page.
#[component]
pub fn PlaylistDetail(id: String) -> Element {
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();
    let id_for_view = id.clone();

    // One resource that fetches the playlist row + its tracks (manual or
    // smart-evaluated). Subscribes to `library_signal.version` so removals
    // and library changes trigger a refresh.
    let view = use_resource(move || {
        let h = handle.clone();
        let pid = id_for_view.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            let playlist = queries::playlists::by_id(h.library.pool(), &pid).await.ok()?;
            let tracks = if playlist.is_smart() {
                let rules: SmartRules = playlist
                    .smart_rules
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| SmartRules {
                        conditions: Vec::new(),
                        combinator: smart::Combinator::And,
                        sort: smart::SortOrder::Default,
                        limit: None,
                    });
                smart::evaluate(h.library.pool(), &rules).await.ok()?
            } else {
                queries::playlists::tracks_of(h.library.pool(), &pid).await.ok()?
            };
            Some(DetailView { playlist, tracks })
        }
    });

    let h_play = use_app_handle();
    let play_all = move |_| {
        let Some(handle) = h_play.clone() else { return; };
        let Some(Some(view)) = view.read_unchecked().as_ref().cloned() else { return; };
        if view.tracks.is_empty() { return; }
        let _ = handle.player.send(sonitus_core::player::commands::PlayerCommand::ClearQueue);
        for t in &view.tracks {
            handle.enqueue(t.id.clone());
        }
        handle.play(view.tracks[0].id.clone());
    };

    let snap = view.read_unchecked().as_ref().cloned().flatten();
    let id_for_edit = id.clone();
    let edit_route = match snap.as_ref() {
        Some(v) if v.playlist.is_smart() => {
            Route::SmartPlaylistEditor { id: id_for_edit.clone() }
        }
        _ => Route::PlaylistEditor { id: id_for_edit.clone() },
    };

    rsx! {
        section { class: "playlist-detail",
            header { class: "playlist-detail__header",
                div { class: "playlist-detail__cover" }
                div { class: "playlist-detail__meta",
                    match snap.as_ref() {
                        Some(v) => {
                            let count = v.tracks.len();
                            let total_ms: i64 = v.tracks
                                .iter()
                                .map(|t| t.duration_ms.unwrap_or(0).max(0))
                                .sum();
                            let kind = if v.playlist.is_smart() { "Smart playlist" } else { "Playlist" };
                            rsx! {
                                div { class: "playlist-detail__kind", "{kind}" }
                                h1 { class: "playlist-detail__title", "{v.playlist.name}" }
                                div { class: "playlist-detail__sub",
                                    "{count} tracks · {format_duration(total_ms as u64)}"
                                }
                                if let Some(desc) = &v.playlist.description {
                                    if !desc.is_empty() {
                                        p { class: "playlist-detail__desc", "{desc}" }
                                    }
                                }
                            }
                        }
                        None => rsx! { h1 { class: "playlist-detail__title", "Playlist" } },
                    }
                    div { class: "playlist-detail__actions",
                        button { class: "btn btn--primary", onclick: play_all, "Play" }
                        Link {
                            to: edit_route,
                            class: "btn btn--ghost",
                            "Edit"
                        }
                        button { class: "btn btn--ghost", "Export M3U8" }
                    }
                }
            }
            ol { class: "playlist-detail__tracks",
                match snap.as_ref() {
                    Some(v) if v.tracks.is_empty() => rsx! {
                        li { class: "playlist-detail__empty",
                            if v.playlist.is_smart() {
                                "No tracks match the current rules. Try editing the rules."
                            } else {
                                "No tracks yet. Add some from the library."
                            }
                        }
                    },
                    Some(v) => rsx! {
                        for t in v.tracks.iter() {
                            PlaylistTrackLine { track: t.clone() }
                        }
                    },
                    None => rsx! {
                        li { class: "playlist-detail__empty", "Loading…" }
                    },
                }
            }
        }
    }
}

#[component]
fn PlaylistTrackLine(track: Track) -> Element {
    let h = use_app_handle();
    let id = track.id.clone();
    let onclick = move |_| {
        if let Some(handle) = h.clone() {
            handle.play(id.clone());
        }
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    rsx! {
        li { class: "playlist-detail__track", ondoubleclick: onclick,
            span { class: "playlist-detail__track-title", "{track.title}" }
            span { class: "playlist-detail__track-dur", "{dur}" }
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
