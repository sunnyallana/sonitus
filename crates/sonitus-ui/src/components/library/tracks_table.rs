//! Sortable track table with artist/album columns + per-row play affordance.

use crate::app::use_app_handle;
use crate::components::playlists::add_to_playlist_dialog::AddToPlaylistState;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries::tracks::TrackView;
use sonitus_core::library::queries;

/// Full table view of all tracks with joined artist + album names.
#[component]
pub fn TracksTable() -> Element {
    let handle = use_app_handle();
    let library_signal = use_context::<Signal<LibraryState>>();

    let tracks = use_resource(move || {
        let h = handle.clone();
        let _version = library_signal.read().version;
        async move {
            let h = h?;
            queries::tracks::recently_added_view(h.library.pool(), 500)
                .await
                .ok()
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
                        th { class: "tracks-table__col tracks-table__col--artist", "Artist" }
                        th { class: "tracks-table__col tracks-table__col--album", "Album" }
                        th { class: "tracks-table__col tracks-table__col--genre", "Genre" }
                        th { class: "tracks-table__col tracks-table__col--year", "Year" }
                        th { class: "tracks-table__col tracks-table__col--duration", "Time" }
                        th { class: "tracks-table__col tracks-table__col--actions", aria_label: "Actions", "" }
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
fn TrackRow(idx: usize, track: TrackView) -> Element {
    let handle = use_app_handle();
    let id1 = track.id.clone();
    let id2 = track.id.clone();
    let id3 = track.id.clone();
    let title_for_dialog = track.title.clone();
    let onclick_play = move |_| {
        if let Some(h) = handle.clone() { h.play(id1.clone()); }
    };
    let h2 = use_app_handle();
    let on_button_play = move |evt: MouseEvent| {
        evt.stop_propagation();
        if let Some(h) = h2.clone() { h.play(id2.clone()); }
    };
    let mut add_state = use_context::<Signal<AddToPlaylistState>>();
    let on_add_to_playlist = move |evt: MouseEvent| {
        evt.stop_propagation();
        add_state.set(AddToPlaylistState {
            open: true,
            track_id: Some(id3.clone()),
            track_title: Some(title_for_dialog.clone()),
            ..AddToPlaylistState::default()
        });
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    let year = track.year.map(|y| y.to_string()).unwrap_or_default();
    let genre = track.genre.clone().unwrap_or_default();
    let artist_name = track.artist_name.clone().unwrap_or_default();
    let album_title = track.album_title.clone().unwrap_or_default();

    rsx! {
        tr { class: "tracks-table__row", ondoubleclick: onclick_play,
            td { class: "tracks-table__num",
                span { class: "tracks-table__num-text", "{idx + 1}" }
                button {
                    class: "tracks-table__play",
                    title: "Play",
                    aria_label: "Play track",
                    onclick: on_button_play,
                    "▶"
                }
            }
            td { class: "tracks-table__title", "{track.title}" }
            td { class: "tracks-table__artist",
                if let Some(aid) = track.artist_id.clone() {
                    Link {
                        to: Route::ArtistDetail { id: aid },
                        class: "tracks-table__link",
                        onclick: |e: MouseEvent| e.stop_propagation(),
                        "{artist_name}"
                    }
                } else {
                    span { "{artist_name}" }
                }
            }
            td { class: "tracks-table__album",
                if let Some(album_id) = track.album_id.clone() {
                    Link {
                        to: Route::AlbumDetail { id: album_id },
                        class: "tracks-table__link",
                        onclick: |e: MouseEvent| e.stop_propagation(),
                        "{album_title}"
                    }
                } else {
                    span { "{album_title}" }
                }
            }
            td { "{genre}" }
            td { class: "tracks-table__year-cell", "{year}" }
            td { class: "tracks-table__time", "{dur}" }
            td { class: "tracks-table__actions",
                button {
                    class: "tracks-table__menu",
                    title: "Add to playlist",
                    aria_label: "Add to playlist",
                    onclick: on_add_to_playlist,
                    "+"
                }
            }
        }
    }
}

#[component]
fn EmptyRow(msg: String) -> Element {
    rsx! {
        tr { class: "tracks-table__row tracks-table__row--empty",
            td { colspan: "8", "{msg}" }
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
