//! Genre cloud / browser backed by the live DB.
//!
//! Clicking a chip filters the results list below to tracks whose
//! `genre` column matches exactly. Click the active chip again to
//! clear the filter.

use crate::app::use_app_handle;
use dioxus::prelude::*;
use sonitus_core::library::Track;
use sonitus_core::library::queries;

/// Genre cloud + filtered tracks below.
#[component]
pub fn GenreBrowser() -> Element {
    let handle = use_app_handle();
    let mut selected = use_signal(|| Option::<String>::None);

    let h_genres = handle.clone();
    let genres = use_resource(move || {
        let h = h_genres.clone();
        async move {
            let h = h?;
            queries::tracks::genres(h.library.pool()).await.ok()
        }
    });

    let h_results = handle.clone();
    let results = use_resource(move || {
        let h = h_results.clone();
        let g = selected.read().clone();
        async move {
            let h = h?;
            let g = g?;
            queries::tracks::by_genre(h.library.pool(), &g).await.ok()
        }
    });

    let snap_selected = selected.read().clone();

    rsx! {
        section { class: "genre-browser",
            header { class: "genre-browser__header",
                h1 { "Genres" }
                if let Some(g) = &snap_selected {
                    span { class: "genre-browser__active",
                        "Filtering by "
                        strong { "{g}" }
                        button {
                            class: "btn btn--ghost btn--sm",
                            onclick: move |_| selected.set(None),
                            "Clear"
                        }
                    }
                }
            }
            div { class: "genre-browser__cloud",
                match &*genres.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        for (g, n) in rows.iter() {
                            {
                                let g_owned = g.clone();
                                let is_active = snap_selected.as_deref() == Some(g.as_str());
                                let chip_class = if is_active {
                                    "genre-chip genre-chip--active"
                                } else {
                                    "genre-chip"
                                };
                                let g_for_click = g_owned.clone();
                                rsx! {
                                    button {
                                        class: chip_class,
                                        onclick: move |_| {
                                            // Toggle: clicking the active chip clears.
                                            let mut sel = selected.write();
                                            if sel.as_deref() == Some(g_for_click.as_str()) {
                                                *sel = None;
                                            } else {
                                                *sel = Some(g_for_click.clone());
                                            }
                                        },
                                        "{g_owned} "
                                        span { class: "genre-chip__count", "({n})" }
                                    }
                                }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "genre-browser__empty", "No genre tags found." }
                    },
                    None => rsx! { p { "Loading…" } },
                }
            }
            div { class: "genre-browser__results",
                match (snap_selected.as_ref(), &*results.read_unchecked()) {
                    (Some(_), Some(Some(rows))) if !rows.is_empty() => rsx! {
                        ol { class: "genre-browser__list",
                            for t in rows.iter() {
                                GenreTrackLine { track: t.clone() }
                            }
                        }
                    },
                    (Some(_), Some(_)) => rsx! {
                        p { class: "genre-browser__empty",
                            "No tracks in this genre yet."
                        }
                    },
                    (Some(_), None) => rsx! { p { "Loading…" } },
                    (None, _) => rsx! {
                        p { class: "genre-browser__hint",
                            "Pick a genre above to see its tracks."
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn GenreTrackLine(track: Track) -> Element {
    let h = use_app_handle();
    let id = track.id.clone();
    let onclick = move |_| {
        if let Some(handle) = h.clone() {
            handle.play(id.clone());
        }
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    rsx! {
        li { class: "genre-browser__row", ondoubleclick: onclick,
            span { class: "genre-browser__row-title", "{track.title}" }
            span { class: "genre-browser__row-dur", "{dur}" }
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
