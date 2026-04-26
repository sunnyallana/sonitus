//! Manual-playlist editor: rename, change description, remove tracks,
//! delete playlist.
//!
//! Smart playlists are read-only here — opening the editor for a smart
//! playlist redirects to `SmartPlaylistEditor` (the rule editor) since
//! their track membership is derived from rules, not editable directly.

use crate::app::use_app_handle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::{Track, queries};

/// Manual-playlist editor — renames metadata and edits track list.
#[component]
pub fn PlaylistEditor(id: String) -> Element {
    let handle = use_app_handle();
    let mut library_signal = use_context::<Signal<LibraryState>>();
    let nav = navigator();

    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loaded = use_signal(|| false);
    let mut is_smart = use_signal(|| false);
    let id_for_load = id.clone();

    // Load metadata once on mount.
    {
        let handle = handle.clone();
        use_effect(move || {
            if *loaded.read() { return; }
            let Some(h) = handle.clone() else { return; };
            let pid = id_for_load.clone();
            dioxus::prelude::spawn(async move {
                match queries::playlists::by_id(h.library.pool(), &pid).await {
                    Ok(p) => {
                        let smart = p.is_smart();
                        name.set(p.name);
                        description.set(p.description.unwrap_or_default());
                        is_smart.set(smart);
                        loaded.set(true);
                    }
                    Err(e) => {
                        error.set(Some(format!("Couldn't load playlist: {e}")));
                        loaded.set(true);
                    }
                }
            });
        });
    }

    // If this is a smart playlist, redirect to the rule editor.
    {
        let id_for_redirect = id.clone();
        use_effect(move || {
            if *is_smart.read() {
                nav.replace(Route::SmartPlaylistEditor { id: id_for_redirect.clone() });
            }
        });
    }

    let id_tracks = id.clone();
    let tracks = use_resource(move || {
        let h = handle.clone();
        let pid = id_tracks.clone();
        let _v = library_signal.read().version;
        async move {
            let h = h?;
            queries::playlists::tracks_of(h.library.pool(), &pid).await.ok()
        }
    });

    // ── Save metadata (name + description) ───────────────────────────────
    let id_save = id.clone();
    let h_save = use_app_handle();
    let on_save = move |_| {
        let Some(h) = h_save.clone() else { return; };
        let pid = id_save.clone();
        let new_name = name.read().trim().to_string();
        if new_name.is_empty() {
            error.set(Some("Name can't be empty.".into()));
            return;
        }
        let desc = description.read().trim().to_string();
        let desc_opt = if desc.is_empty() { None } else { Some(desc) };
        dioxus::prelude::spawn(async move {
            let pool = h.library.pool();
            if let Err(e) = queries::playlists::rename(pool, &pid, &new_name).await {
                error.set(Some(format!("Rename failed: {e}")));
                return;
            }
            if let Err(e) =
                queries::playlists::set_description(pool, &pid, desc_opt.as_deref()).await
            {
                error.set(Some(format!("Description update failed: {e}")));
                return;
            }
            error.set(None);
            let next = library_signal.peek().version.wrapping_add(1);
            library_signal.write().version = next;
        });
    };

    // ── Delete the playlist ──────────────────────────────────────────────
    let id_delete = id.clone();
    let h_delete = use_app_handle();
    let on_delete = move |_| {
        let Some(h) = h_delete.clone() else { return; };
        let pid = id_delete.clone();
        dioxus::prelude::spawn(async move {
            match queries::playlists::delete(h.library.pool(), &pid).await {
                Ok(_) => {
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                    nav.replace(Route::PlaylistsList {});
                }
                Err(e) => error.set(Some(format!("Delete failed: {e}"))),
            }
        });
    };

    // ── Cancel back to detail ────────────────────────────────────────────
    let id_cancel = id.clone();
    let on_cancel = move |_| {
        nav.replace(Route::PlaylistDetail { id: id_cancel.clone() });
    };

    let snap_name = name.read().clone();
    let snap_desc = description.read().clone();
    let snap_error = error.read().clone();

    rsx! {
        section { class: "playlist-editor",
            header { class: "playlist-editor__header",
                h1 { "Edit playlist" }
            }
            label { class: "field",
                span { class: "field__label", "Name" }
                input {
                    r#type: "text",
                    class: "input",
                    value: "{snap_name}",
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
            }
            label { class: "field",
                span { class: "field__label", "Description" }
                textarea {
                    class: "input input--multiline",
                    rows: "3",
                    value: "{snap_desc}",
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
            }

            if let Some(err) = &snap_error {
                p { class: "wizard__error", "{err}" }
            }

            div { class: "playlist-editor__tracks",
                h2 { "Tracks" }
                match &*tracks.read_unchecked() {
                    Some(Some(rows)) if !rows.is_empty() => rsx! {
                        ul { class: "playlist-editor__list",
                            for t in rows.iter() {
                                EditableTrackRow {
                                    playlist_id: id.clone(),
                                    track: t.clone(),
                                }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        p { class: "playlist-editor__empty",
                            "No tracks yet. Add some from the library."
                        }
                    },
                    None => rsx! { p { class: "playlist-editor__empty", "Loading…" } },
                }
            }

            div { class: "playlist-editor__actions",
                button { class: "btn btn--ghost", onclick: on_cancel, "Cancel" }
                div { class: "playlist-editor__actions-right",
                    button { class: "btn btn--primary", onclick: on_save, "Save" }
                    button { class: "btn btn--danger", onclick: on_delete, "Delete playlist" }
                }
            }
        }
    }
}

#[component]
fn EditableTrackRow(playlist_id: String, track: Track) -> Element {
    let handle = use_app_handle();
    let mut library_signal = use_context::<Signal<LibraryState>>();
    let track_id = track.id.clone();
    let on_remove = move |_| {
        let Some(h) = handle.clone() else { return; };
        let pid = playlist_id.clone();
        let tid = track_id.clone();
        dioxus::prelude::spawn(async move {
            if queries::playlists::remove_track(h.library.pool(), &pid, &tid)
                .await
                .is_ok()
            {
                let next = library_signal.peek().version.wrapping_add(1);
                library_signal.write().version = next;
            }
        });
    };
    let dur = format_duration(track.duration_ms.unwrap_or(0).max(0) as u64);
    rsx! {
        li { class: "playlist-editor__row",
            span { class: "playlist-editor__row-title", "{track.title}" }
            span { class: "playlist-editor__row-dur", "{dur}" }
            button {
                class: "btn btn--ghost btn--icon",
                title: "Remove from playlist",
                aria_label: "Remove from playlist",
                onclick: on_remove,
                "×"
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
