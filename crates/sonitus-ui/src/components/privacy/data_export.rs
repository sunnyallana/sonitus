//! Data export controls.
//!
//! Lets the user download:
//! - The `.sonitus` library file
//! - A CSV of every track row
//! - A zip of all extracted cover art
//! - The audit log

use dioxus::prelude::*;

/// Data export page.
#[component]
pub fn DataExport() -> Element {
    rsx! {
        section { class: "data-export",
            h1 { "Export your data" }
            p {
                "Sonitus belongs to you. Export it any time, in formats you can read."
            }
            ul { class: "data-export__items",
                li {
                    h3 { "Library file (.sonitus)" }
                    p { "TOML file with sources, playlists, and preferences. No secrets." }
                    button { class: "btn btn--primary", "Export .sonitus" }
                }
                li {
                    h3 { "Track list (CSV)" }
                    p { "One row per track, with all tag data and play counts." }
                    button { class: "btn btn--primary", "Export tracks.csv" }
                }
                li {
                    h3 { "Cover art (zip)" }
                    p { "All extracted album cover art." }
                    button { class: "btn btn--primary", "Export covers.zip" }
                }
                li {
                    h3 { "Audit log (JSONL)" }
                    p { "Every outbound request Sonitus has made." }
                    button { class: "btn btn--primary", "Export audit.log" }
                }
            }
        }
    }
}
