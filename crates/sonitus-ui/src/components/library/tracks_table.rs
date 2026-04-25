//! Sortable track table with context-menu.

use dioxus::prelude::*;

/// Full table view of all tracks.
#[component]
pub fn TracksTable() -> Element {
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
                    }
                }
                tbody { class: "tracks-table__body" }
            }
        }
    }
}
