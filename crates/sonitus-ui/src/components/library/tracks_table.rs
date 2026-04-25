//! Sortable track table with context-menu.

use crate::app::use_app_handle;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::{Track, queries};

/// Full table view of all tracks.
#[component]
pub fn TracksTable() -> Element {
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    let tracks = use_resource(move || {
        let h = handle.clone();
        // Subscribe to library_version inside the closure so this
        // resource re-runs whenever the orchestrator bumps it (e.g.
        // duration backfill, scan completion).
        let _version = library_signal.read().version;
        async move {
            let h = h?;
            queries::tracks::recently_added(h.library.pool(), 500).await.ok()
        }
    });

    rsx! {
        section { class: "tracks-table",
            header { class: "tracks-table__header",
                h1 { "Tracks" }
            }
            table { class: "tracks-table__grid", role: "grid",
                thead {
                    tr {
                        th { class: "tracks-table__col tracks-table__col--num", "#" }
                        th { class: "tracks-table__col tracks-table__col--title", "Title" }
                        th { class: "tracks-table__col tracks-table__col--genre", "Genre" }
                        th { class: "tracks-table__col tracks-table__col--year", "Year" }
                        th { class: "tracks-table__col tracks-table__col--duration", "Time" }
                    }
                }
                tbody { class: "tracks-table__body",
                    match &*tracks.read_unchecked() {
                        Some(Some(rows)) => rsx! {
                            for (i, t) in rows.iter().enumerate() {
                                TrackRow { idx: i, track: t.clone() }
                            }
                        },
                        Some(None) => rsx! { EmptyRow { msg: String::from("No tracks yet — add a source.") } },
                        None => rsx! { EmptyRow { msg: String::from("Loading...") } },
                    }
                }
            }
        }
    }
}

#[component]
fn TrackRow(idx: usize, track: Track) -> Element {
    let handle = use_app_handle();
    let id1 = track.id.clone();
    let id2 = track.id.clone();
    let onclick_play = move |_| {
        if let Some(h) = handle.clone() {
            h.play(id1.clone());
        }
    };
    let h2 = use_app_handle();
    let on_button_play = move |evt: MouseEvent| {
        evt.stop_propagation();
        if let Some(h) = h2.clone() {
            h.play(id2.clone());
        }
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    let year = track.year.map(|y| y.to_string()).unwrap_or_default();
    let genre = track.genre.clone().unwrap_or_default();

    rsx! {
        tr { class: "tracks-table__row", ondoubleclick: onclick_play,
            td { class: "tracks-table__num",
                // Number is hidden on hover and replaced with the play button.
                span { class: "tracks-table__num-text", "{idx + 1}" }
                button { class: "tracks-table__play",
                    title: "Play",
                    aria_label: "Play track",
                    onclick: on_button_play,
                    "▶"
                }
            }
            td { class: "tracks-table__title", "{track.title}" }
            td { "{genre}" }
            td { "{year}" }
            td { class: "tracks-table__time", "{dur}" }
        }
    }
}

#[component]
fn EmptyRow(msg: String) -> Element {
    rsx! {
        tr { class: "tracks-table__row tracks-table__row--empty",
            td { colspan: "5", "{msg}" }
        }
    }
}

fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}
