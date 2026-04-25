//! Playlist detail — header + track list + edit button.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::{Track, queries};

/// Playlist detail page.
#[component]
pub fn PlaylistDetail(id: String) -> Element {
    let handle = use_app_handle();
    let id_meta = id.clone();
    let id_tracks = id.clone();

    let meta = use_resource(move || {
        let h = handle.clone();
        let pid = id_meta.clone();
        async move {
            let h = h?;
            queries::playlists::by_id(h.library.pool(), &pid).await.ok()
        }
    });

    let h2 = use_app_handle();
    let tracks = use_resource(move || {
        let h = h2.clone();
        let pid = id_tracks.clone();
        async move {
            let h = h?;
            queries::playlists::tracks_of(h.library.pool(), &pid).await.ok()
        }
    });

    let h_play = use_app_handle();
    let play_all = move |_| {
        let Some(handle) = h_play.clone() else { return; };
        let Some(Some(rows)) = tracks.read_unchecked().as_ref().cloned() else { return; };
        if rows.is_empty() { return; }
        let _ = handle.player.send(sonitus_core::player::commands::PlayerCommand::ClearQueue);
        for t in &rows {
            handle.enqueue(t.id.clone());
        }
        handle.play(rows[0].id.clone());
    };

    rsx! {
        section { class: "playlist-detail",
            header { class: "playlist-detail__header",
                div { class: "playlist-detail__cover" }
                div { class: "playlist-detail__meta",
                    match &*meta.read_unchecked() {
                        Some(Some(p)) => rsx! {
                            h1 { class: "playlist-detail__title", "{p.name}" }
                            div { class: "playlist-detail__sub",
                                "{p.track_count} tracks · {format_duration(p.total_duration_ms as u64)}"
                            }
                        },
                        _ => rsx! { h1 { class: "playlist-detail__title", "Playlist" } },
                    }
                    div { class: "playlist-detail__actions",
                        button { class: "btn btn--primary", onclick: play_all, "Play" }
                        button { class: "btn btn--ghost", "Edit" }
                        button { class: "btn btn--ghost", "Export M3U8" }
                    }
                }
            }
            ol { class: "playlist-detail__tracks",
                match &*tracks.read_unchecked() {
                    Some(Some(rows)) => rsx! {
                        for t in rows.iter() {
                            PlaylistTrackLine { track: t.clone() }
                        }
                    },
                    _ => rsx! {},
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
        li { class: "playlist-detail__track", ondblclick: onclick,
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
