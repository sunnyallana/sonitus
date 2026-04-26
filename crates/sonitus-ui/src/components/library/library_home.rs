//! Library home page — recently added, recently played, smart playlists.

use crate::app::use_app_handle;
use crate::hooks::use_library::use_library;
use crate::routes::Route;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Library home — landing page after first launch.
#[component]
pub fn LibraryHome() -> Element {
    let library = use_library();
    let state = library.read();

    let h_recent = use_app_handle();
    let recent = use_resource(move || {
        let h = h_recent.clone();
        async move {
            let h = h?;
            queries::tracks::recently_added(h.library.pool(), 12).await.ok()
        }
    });

    let h_played = use_app_handle();
    let played = use_resource(move || {
        let h = h_played.clone();
        async move {
            let h = h?;
            queries::tracks::recently_played(h.library.pool(), 12).await.ok()
        }
    });

    let h_smart = use_app_handle();
    let smart_playlists = use_resource(move || {
        let h = h_smart.clone();
        async move {
            let h = h?;
            queries::playlists::list_all(h.library.pool())
                .await
                .ok()
                .map(|all| all.into_iter().filter(|p| p.is_smart()).collect::<Vec<_>>())
        }
    });

    rsx! {
        section { class: "library-home",
            header { class: "library-home__header",
                h1 { "Your library" }
                p { class: "library-home__summary",
                    "{state.track_count} tracks · {state.album_count} albums · {state.artist_count} artists"
                }
            }
            div { class: "library-home__grid",
                NavCard { title: "Tracks", to: Route::TracksTable {}, count: state.track_count }
                NavCard { title: "Albums", to: Route::AlbumsGrid {}, count: state.album_count }
                NavCard { title: "Artists", to: Route::ArtistsList {}, count: state.artist_count }
                NavCard { title: "Playlists", to: Route::PlaylistsList {}, count: state.playlist_count }
            }

            section { class: "library-home__section",
                h2 { "Recently added" }
                ul { class: "library-home__recent",
                    match &*recent.read_unchecked() {
                        Some(Some(rows)) if !rows.is_empty() => rsx! {
                            for t in rows.iter() {
                                TrackQuickRow { track_id: t.id.clone(), title: t.title.clone() }
                            }
                        },
                        Some(_) => rsx! {
                            li { class: "library-home__empty",
                                "Nothing added yet — try adding a source."
                            }
                        },
                        None => rsx! { li { class: "library-home__empty", "Loading…" } },
                    }
                }
            }

            section { class: "library-home__section",
                h2 { "Recently played" }
                ul { class: "library-home__recent",
                    match &*played.read_unchecked() {
                        Some(Some(rows)) if !rows.is_empty() => rsx! {
                            for t in rows.iter() {
                                TrackQuickRow { track_id: t.id.clone(), title: t.title.clone() }
                            }
                        },
                        Some(_) => rsx! {
                            li { class: "library-home__empty",
                                "Nothing played yet."
                            }
                        },
                        None => rsx! { li { class: "library-home__empty", "Loading…" } },
                    }
                }
            }

            section { class: "library-home__section",
                h2 { "Smart playlists" }
                div { class: "library-home__smart",
                    match &*smart_playlists.read_unchecked() {
                        Some(Some(rows)) if !rows.is_empty() => rsx! {
                            for p in rows.iter() {
                                Link {
                                    to: Route::PlaylistDetail { id: p.id.clone() },
                                    class: "smart-card",
                                    div { class: "smart-card__title", "{p.name}" }
                                    div { class: "smart-card__count", "{p.track_count} tracks" }
                                }
                            }
                        },
                        Some(_) => rsx! {
                            p { class: "library-home__empty",
                                "No smart playlists yet. Create one from the Playlists page."
                            }
                        },
                        None => rsx! { p { class: "library-home__empty", "Loading…" } },
                    }
                }
            }

            if state.track_count == 0 {
                EmptyState {}
            }
        }
    }
}

#[component]
fn TrackQuickRow(track_id: String, title: String) -> Element {
    let handle = use_app_handle();
    let id_for_play = track_id.clone();
    let onclick = move |_| {
        if let Some(h) = handle.clone() {
            h.play(id_for_play.clone());
        }
    };
    rsx! {
        li { class: "recent-row", key: "{track_id}", ondoubleclick: onclick,
            span { class: "recent-row__title", "{title}" }
        }
    }
}

#[component]
fn NavCard(title: String, to: Route, count: u64) -> Element {
    rsx! {
        Link { to: to, class: "nav-card",
            div { class: "nav-card__title", "{title}" }
            div { class: "nav-card__count", "{count}" }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    rsx! {
        div { class: "empty-state",
            h2 { "Add a source to get started" }
            p { "Sonitus indexes music wherever you keep it. Add a folder, a Google Drive, an S3 bucket, or your home NAS." }
            Link { to: Route::SourcesList {}, class: "empty-state__cta", "Add a source" }
        }
    }
}
