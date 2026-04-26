//! Smart-playlist rule editor.
//!
//! Adds/removes rule rows (field + op + value), picks combinator and
//! sort, previews matching tracks live via `playlist::smart::evaluate`.
//! Routed at `/playlists/smart/:id` — `id == "new"` creates a fresh
//! playlist, anything else loads the existing row and saves over it.

use crate::app::use_app_handle;
use crate::routes::Route;
use crate::state::library_state::LibraryState;
use dioxus::prelude::*;
use sonitus_core::library::queries;
use sonitus_core::library::Track;
use sonitus_core::playlist::smart::{
    Combinator, SmartCondition, SmartField, SmartOp, SmartRules, SortOrder,
};

#[derive(Debug, Clone, PartialEq)]
struct EditorState {
    name: String,
    description: String,
    conditions: Vec<EditableCondition>,
    combinator: Combinator,
    sort: SortOrder,
    limit: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct EditableCondition {
    field: SmartField,
    op: SmartOp,
    value: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            conditions: vec![EditableCondition {
                field: SmartField::Genre,
                op: SmartOp::Eq,
                value: String::new(),
            }],
            combinator: Combinator::And,
            sort: SortOrder::Default,
            limit: None,
            error: None,
        }
    }
}

impl EditorState {
    fn to_rules(&self) -> SmartRules {
        let conditions: Vec<SmartCondition> = self
            .conditions
            .iter()
            .filter(|c| !c.value.trim().is_empty())
            .map(|c| {
                let value = if matches!(
                    c.field,
                    SmartField::Year
                        | SmartField::Bpm
                        | SmartField::Rating
                        | SmartField::PlayCount
                        | SmartField::DurationMs
                        | SmartField::LastPlayedAt
                        | SmartField::CreatedAt
                ) {
                    serde_json::Value::Number(c.value.trim().parse().unwrap_or(0.into()))
                } else if matches!(c.field, SmartField::Loved) {
                    serde_json::Value::Bool(c.value.trim().eq_ignore_ascii_case("true"))
                } else {
                    serde_json::Value::String(c.value.trim().to_string())
                };
                SmartCondition { field: c.field, op: c.op, value }
            })
            .collect();
        SmartRules {
            conditions,
            combinator: self.combinator,
            sort: self.sort,
            limit: self.limit,
        }
    }

    /// Reverse of `to_rules` — populate the editable rows from a parsed
    /// `SmartRules`. JSON values are stringified for display in the
    /// `<input type="text">` cells; numbers and booleans round-trip through
    /// `to_rules` correctly.
    fn from_existing(name: String, description: Option<String>, rules: SmartRules) -> Self {
        let conditions = if rules.conditions.is_empty() {
            // The editor always shows at least one row so users have
            // somewhere to type.
            vec![EditableCondition {
                field: SmartField::Genre,
                op: SmartOp::Eq,
                value: String::new(),
            }]
        } else {
            rules
                .conditions
                .into_iter()
                .map(|c| {
                    let value = match c.value {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        other => other.to_string(),
                    };
                    EditableCondition { field: c.field, op: c.op, value }
                })
                .collect()
        };
        Self {
            name,
            description: description.unwrap_or_default(),
            conditions,
            combinator: rules.combinator,
            sort: rules.sort,
            limit: rules.limit,
            error: None,
        }
    }
}

/// Smart-playlist rule editor + live preview.
#[component]
pub fn SmartPlaylistEditor(id: String) -> Element {
    let mut editor = use_signal(EditorState::default);
    let mut loaded = use_signal(|| false);
    let handle = use_app_handle();
    let mut library_signal = use_context::<Signal<LibraryState>>();
    let nav = navigator();

    let is_new = id == "new";
    let editing_id = if is_new { None } else { Some(id.clone()) };

    // On mount, if we're editing an existing playlist, fetch its row and
    // populate the editor state. Re-runs only if `id` changes.
    {
        let handle = handle.clone();
        let editing_id = editing_id.clone();
        use_effect(move || {
            let Some(playlist_id) = editing_id.clone() else {
                loaded.set(true);
                return;
            };
            if *loaded.read() { return; }
            let Some(h) = handle.clone() else { return; };
            dioxus::prelude::spawn(async move {
                match queries::playlists::by_id(h.library.pool(), &playlist_id).await {
                    Ok(row) if row.is_smart() => {
                        let rules: SmartRules = row
                            .smart_rules
                            .as_deref()
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or_else(|| SmartRules {
                                conditions: Vec::new(),
                                combinator: Combinator::And,
                                sort: SortOrder::Default,
                                limit: None,
                            });
                        editor.set(EditorState::from_existing(
                            row.name,
                            row.description,
                            rules,
                        ));
                        loaded.set(true);
                    }
                    Ok(_) => {
                        editor.write().error = Some(
                            "This playlist isn't a smart playlist.".into()
                        );
                        loaded.set(true);
                    }
                    Err(e) => {
                        editor.write().error = Some(format!("Couldn't load playlist: {e}"));
                        loaded.set(true);
                    }
                }
            });
        });
    }

    // Live preview of matching tracks. Held back until initial load
    // finishes so we don't preview the empty default state on top of an
    // existing-playlist edit.
    let preview = {
        let handle = handle.clone();
        use_resource(move || {
            let h = handle.clone();
            let ready = *loaded.read();
            let rules = editor.read().to_rules();
            async move {
                if !ready { return None; }
                let h = h?;
                sonitus_core::playlist::smart::evaluate(h.library.pool(), &rules)
                    .await
                    .ok()
            }
        })
    };

    let editing_id_save = editing_id.clone();
    let on_save = move |_| {
        let Some(handle) = handle.clone() else { return; };
        let snap = editor.read().clone();
        if snap.name.trim().is_empty() {
            editor.write().error = Some("Name can't be empty.".into());
            return;
        }
        let rules = snap.to_rules();
        let rules_json = match serde_json::to_string(&rules) {
            Ok(j) => j,
            Err(e) => {
                editor.write().error = Some(format!("Couldn't serialize rules: {e}"));
                return;
            }
        };
        let name = snap.name.clone();
        let desc = if snap.description.trim().is_empty() { None } else { Some(snap.description.clone()) };
        let editing_id = editing_id_save.clone();
        dioxus::prelude::spawn(async move {
            let pool = handle.library.pool();
            let res = match editing_id.clone() {
                Some(pid) => queries::playlists::update_smart(
                    pool,
                    &pid,
                    &name,
                    desc.as_deref(),
                    &rules_json,
                )
                .await
                .map(|_| pid),
                None => queries::playlists::create_smart(
                    pool,
                    &name,
                    desc.as_deref(),
                    &rules_json,
                )
                .await
                .map(|p| p.id),
            };
            match res {
                Ok(saved_id) => {
                    let next = library_signal.peek().version.wrapping_add(1);
                    library_signal.write().version = next;
                    nav.replace(Route::PlaylistDetail { id: saved_id });
                }
                Err(e) => editor.write().error = Some(format!("Save failed: {e}")),
            }
        });
    };

    let snap = editor.read().clone();
    let title = if is_new { "New smart playlist" } else { "Edit smart playlist" };

    rsx! {
        section { class: "smart-editor",
            h1 { "{title}" }

            label { class: "field",
                span { class: "field__label", "Name" }
                input {
                    r#type: "text",
                    class: "input",
                    placeholder: "e.g. High-energy rock",
                    value: "{snap.name}",
                    oninput: move |e: FormEvent| editor.write().name = e.value(),
                }
            }
            label { class: "field",
                span { class: "field__label", "Description (optional)" }
                input {
                    r#type: "text",
                    class: "input",
                    placeholder: "What this playlist is for",
                    value: "{snap.description}",
                    oninput: move |e: FormEvent| editor.write().description = e.value(),
                }
            }

            div { class: "smart-editor__rules",
                div { class: "smart-editor__rules-head",
                    h2 { "Rules" }
                    select { class: "select select--small",
                        value: combinator_to_str(snap.combinator),
                        onchange: move |e: FormEvent| {
                            editor.write().combinator = match e.value().as_str() {
                                "or" => Combinator::Or,
                                _ => Combinator::And,
                            };
                        },
                        option { value: "and", "Match all (AND)" }
                        option { value: "or", "Match any (OR)" }
                    }
                }
                ul { class: "smart-editor__list",
                    for (idx, cond) in snap.conditions.iter().enumerate() {
                        ConditionRow {
                            idx: idx,
                            cond: cond.clone(),
                            editor: editor,
                        }
                    }
                }
                button {
                    class: "btn btn--ghost",
                    onclick: move |_| {
                        editor.write().conditions.push(EditableCondition {
                            field: SmartField::Genre,
                            op: SmartOp::Eq,
                            value: String::new(),
                        });
                    },
                    "+ Add rule"
                }
            }

            div { class: "smart-editor__sort-row",
                label { class: "field field--inline",
                    span { class: "field__label", "Sort" }
                    select {
                        class: "select select--small",
                        value: sort_to_str(snap.sort),
                        onchange: move |e: FormEvent| {
                            editor.write().sort = match e.value().as_str() {
                                "recently_added" => SortOrder::RecentlyAdded,
                                "recently_played" => SortOrder::RecentlyPlayed,
                                "most_played" => SortOrder::MostPlayed,
                                "random" => SortOrder::Random,
                                _ => SortOrder::Default,
                            };
                        },
                        option { value: "default",         "Default order" }
                        option { value: "recently_added",  "Recently added" }
                        option { value: "recently_played", "Recently played" }
                        option { value: "most_played",     "Most played" }
                        option { value: "random",          "Random" }
                    }
                }
                label { class: "field field--inline",
                    span { class: "field__label", "Limit" }
                    input {
                        r#type: "number",
                        class: "input input--small",
                        min: "0",
                        placeholder: "0 = unlimited",
                        value: "{snap.limit.unwrap_or(0)}",
                        oninput: move |e: FormEvent| {
                            let n: i64 = e.value().trim().parse().unwrap_or(0);
                            editor.write().limit = if n > 0 { Some(n) } else { None };
                        }
                    }
                }
            }

            if let Some(err) = &snap.error {
                p { class: "wizard__error", "{err}" }
            }

            div { class: "smart-editor__preview",
                h2 { "Matching tracks (preview)" }
                match &*preview.read_unchecked() {
                    Some(Some(rows)) => rsx! {
                        p { class: "smart-editor__preview-count", "{rows.len()} matches" }
                        ol { class: "smart-editor__preview-list",
                            for t in rows.iter().take(20) {
                                PreviewLine { track: t.clone() }
                            }
                            if rows.len() > 20 {
                                li { class: "smart-editor__preview-ellipsis",
                                    "...and {rows.len() - 20} more"
                                }
                            }
                        }
                    },
                    _ => rsx! { p { "Loading preview..." } }
                }
            }

            div { class: "wizard__footer",
                button {
                    class: "btn btn--primary",
                    onclick: on_save,
                    if is_new { "Create smart playlist" } else { "Save changes" }
                }
            }
        }
    }
}

#[component]
fn ConditionRow(idx: usize, cond: EditableCondition, editor: Signal<EditorState>) -> Element {
    let mut editor = editor;
    rsx! {
        li { class: "smart-editor__rule",
            select {
                class: "select select--small",
                onchange: move |e: FormEvent| {
                    if let Ok(f) = field_from_str(&e.value()) {
                        editor.write().conditions[idx].field = f;
                    }
                },
                option { value: "genre",         "Genre" }
                option { value: "artist_name",   "Artist" }
                option { value: "album_title",   "Album" }
                option { value: "year",          "Year" }
                option { value: "bpm",           "BPM" }
                option { value: "rating",        "Rating" }
                option { value: "loved",         "Loved" }
                option { value: "play_count",    "Play count" }
                option { value: "duration_ms",   "Duration (ms)" }
                option { value: "format",        "Format" }
            }
            select {
                class: "select select--small",
                onchange: move |e: FormEvent| {
                    if let Ok(o) = op_from_str(&e.value()) {
                        editor.write().conditions[idx].op = o;
                    }
                },
                option { value: "eq",          "equals" }
                option { value: "ne",          "not equal" }
                option { value: "lt",          "<" }
                option { value: "lte",         "<=" }
                option { value: "gt",          ">" }
                option { value: "gte",         ">=" }
                option { value: "contains",    "contains" }
                option { value: "starts_with", "starts with" }
            }
            input {
                r#type: "text",
                class: "input input--small",
                placeholder: "value",
                value: "{cond.value}",
                oninput: move |e: FormEvent| { editor.write().conditions[idx].value = e.value(); },
            }
            button {
                class: "btn btn--ghost btn--icon",
                title: "Remove rule",
                onclick: move |_| { editor.write().conditions.remove(idx); },
                "×"
            }
        }
    }
}

#[component]
fn PreviewLine(track: Track) -> Element {
    rsx! {
        li { class: "smart-editor__preview-row",
            span { class: "smart-editor__preview-title", "{track.title}" }
            if let Some(g) = &track.genre {
                span { class: "smart-editor__preview-genre", "{g}" }
            }
        }
    }
}

fn combinator_to_str(c: Combinator) -> &'static str {
    match c {
        Combinator::And => "and",
        Combinator::Or => "or",
    }
}

fn sort_to_str(s: SortOrder) -> &'static str {
    match s {
        SortOrder::Default => "default",
        SortOrder::RecentlyAdded => "recently_added",
        SortOrder::RecentlyPlayed => "recently_played",
        SortOrder::MostPlayed => "most_played",
        SortOrder::Random => "random",
    }
}

/// Detour through serde_json to parse the serde-renamed lowercase strings
/// of `SmartField` / `SmartOp`. (We could implement FromStr explicitly but
/// this avoids redefining the case mappings.)
fn field_from_str(s: &str) -> Result<SmartField, ()> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| ())
}

fn op_from_str(s: &str) -> Result<SmartOp, ()> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| ())
}
