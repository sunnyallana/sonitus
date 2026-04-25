//! Album detail — cover art, metadata, full track list.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::{Track, queries};

/// Album detail page.
#[component]
pub fn AlbumDetail(id: String) -> Element {
    let handle = use_app_handle();
    let id_for_album = id.clone();
    let id_for_tracks = id.clone();

    let album = use_resource(move || {
        let h = handle.clone();
        let aid = id_for_album.clone();
        async move {
            let h = h?;
            queries::albums::by_id(h.library.pool(), &aid).await.ok()
        }
    });
    let h_for_tracks = use_app_handle();
    let tracks = use_resource(move || {
        let h = h_for_tracks.clone();
        let aid = id_for_tracks.clone();
        async move {
            let h = h?;
            queries::tracks::by_album(h.library.pool(), &aid).await.ok()
        }
    });

    let h_for_play = use_app_handle();
    let play_album = move |_| {
        let Some(handle) = h_for_play.clone() else { return; };
        let snapshot = tracks.read_unchecked().as_ref().cloned();
        let Some(Some(rows)) = snapshot else { return; };
        if rows.is_empty() { return; }
        let _ = handle.player.send(sonitus_core::player::commands::PlayerCommand::ClearQueue);
        for t in &rows {
            handle.enqueue(t.id.clone());
        }
        handle.play(rows[0].id.clone());
    };

    rsx! {
        section { class: "album-detail",
            header { class: "album-detail__header",
                div { class: "album-detail__cover" }
                div { class: "album-detail__meta",
                    match &*album.read_unchecked() {
                        Some(Some(a)) => rsx! {
                            h1 { class: "album-detail__title", "{a.title}" }
                            p { class: "album-detail__year",
                                "{a.year.map(|y| y.to_string()).unwrap_or_default()} · "
                                "{a.genre.clone().unwrap_or_default()}"
                            }
                        },
                        _ => rsx! { h1 { class: "album-detail__title", "Album" } },
                    }
                    div { class: "album-detail__actions",
                        button { class: "btn btn--primary", onclick: play_album, "Play album" }
                    }
                }
            }
            ol { class: "album-detail__tracks",
                match &*tracks.read_unchecked() {
                    Some(Some(rows)) => rsx! {
                        for t in rows.iter() {
                            TrackLine { track: t.clone() }
                        }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}

#[component]
fn TrackLine(track: Track) -> Element {
    let h = use_app_handle();
    let id = track.id.clone();
    let onclick = move |_| {
        if let Some(handle) = h.clone() {
            handle.play(id.clone());
        }
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    rsx! {
        li { class: "album-detail__track", ondoubleclick: onclick,
            span { class: "album-detail__track-num",
                "{track.track_number.unwrap_or(0)}"
            }
            span { class: "album-detail__track-title", "{track.title}" }
            span { class: "album-detail__track-dur", "{dur}" }
        }
    }
}

fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}
