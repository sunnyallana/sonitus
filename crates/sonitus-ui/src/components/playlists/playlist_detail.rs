//! Playlist detail — header + track list + edit button.
//!
//! Resolves tracks differently based on playlist type:
//! - **Manual**: reads `playlist_tracks` rows ordered by position.
//! - **Smart**: parses `smart_rules` JSON and runs the rule engine
//!   (`playlist::smart::evaluate`) so the displayed tracks always reflect
//!   current library state — never a stale snapshot.

use crate::app::use_app_handle;
use crate::orchestrator::AppHandle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::{Playlist, queries};
use sonitus_core::library::Track;
use sonitus_core::library::queries::artists;
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

    let mut export_status = use_signal(|| Option::<String>::None);
    let h_export = use_app_handle();
    let on_export = move |_| {
        let Some(handle) = h_export.clone() else { return; };
        let Some(Some(view)) = view.read_unchecked().as_ref().cloned() else { return; };
        if view.tracks.is_empty() {
            export_status.set(Some("Nothing to export — playlist is empty.".into()));
            return;
        }
        let playlist_name = view.playlist.name.clone();
        let tracks = view.tracks.clone();
        dioxus::prelude::spawn(async move {
            match export_m3u8(&handle, &playlist_name, &tracks).await {
                Ok(Some(path)) => export_status.set(Some(format!(
                    "Exported {} tracks to {}", tracks.len(), path.display()
                ))),
                Ok(None) => {} // user cancelled the dialog
                Err(e) => export_status.set(Some(format!("Export failed: {e}"))),
            }
        });
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
                        button { class: "btn btn--ghost", onclick: on_export, "Export M3U8" }
                    }
                    if let Some(msg) = export_status.read().as_ref() {
                        p { class: "wizard__success", "{msg}" }
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

/// Generate an extended M3U8 file for `tracks` and prompt the user to
/// save it via the native file dialog. Returns `Ok(None)` if the user
/// cancels the dialog, `Ok(Some(path))` on success.
///
/// Path resolution order per track:
/// 1. `local_cache_path` if set (cached download).
/// 2. The source provider's `local_path(remote_path)` (works for local
///    sources; cloud providers return `None`).
/// 3. The raw `remote_path` as a fallback — most useful when the
///    playlist is opened later by an external player that knows the
///    user's cloud mounts.
async fn export_m3u8(
    handle: &AppHandle,
    playlist_name: &str,
    tracks: &[Track],
) -> Result<Option<std::path::PathBuf>, String> {
    // Resolve display + path strings for every track BEFORE prompting,
    // so the dialog isn't fighting for the rfd thread with our DB calls.
    let mut entries: Vec<(i64, String, String)> = Vec::with_capacity(tracks.len());
    for t in tracks {
        let dur_secs = (t.duration_ms.unwrap_or(0).max(0) / 1000) as i64;
        let artist_name = match &t.artist_id {
            Some(aid) => artists::by_id(handle.library.pool(), aid)
                .await
                .ok()
                .map(|a| a.name)
                .unwrap_or_default(),
            None => String::new(),
        };
        let label = if artist_name.is_empty() {
            t.title.clone()
        } else {
            format!("{} - {}", artist_name, t.title)
        };

        let path = t
            .local_cache_path
            .clone()
            .or_else(|| {
                handle
                    .sources
                    .get(&t.source_id)
                    .and_then(|p| p.local_path(&t.remote_path))
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| t.remote_path.clone());

        entries.push((dur_secs, label, path));
    }

    let suggested = sanitize_filename(playlist_name);

    #[cfg(not(target_arch = "wasm32"))]
    let dest = {
        let suggested = format!("{suggested}.m3u8");
        rfd::FileDialog::new()
            .set_file_name(&suggested)
            .add_filter("M3U8 playlist", &["m3u8", "m3u"])
            .save_file()
    };
    #[cfg(target_arch = "wasm32")]
    let dest: Option<std::path::PathBuf> = None;

    let Some(mut dest) = dest else { return Ok(None); };
    if dest.extension().is_none() {
        dest.set_extension("m3u8");
    }

    let mut out = String::with_capacity(64 + entries.len() * 96);
    out.push_str("#EXTM3U\n");
    for (dur, label, path) in &entries {
        out.push_str(&format!("#EXTINF:{dur},{label}\n"));
        out.push_str(path);
        out.push('\n');
    }

    std::fs::write(&dest, out).map_err(|e| e.to_string())?;
    Ok(Some(dest))
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() { "playlist".into() } else { trimmed.to_string() }
}
