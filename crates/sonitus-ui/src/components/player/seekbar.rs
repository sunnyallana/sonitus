//! Scrub bar showing playback progress.

use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Seekbar — shows played/buffered/remaining.
#[component]
pub fn Seekbar() -> Element {
    let player = use_player();
    let pct = player.progress_fraction() * 100.0;
    let state = player.read();
    let cur = format_time(state.position_ms);
    let total = format_time(state.duration_ms);

    rsx! {
        div { class: "seekbar",
            span { class: "seekbar__time seekbar__time--cur", "{cur}" }
            div { class: "seekbar__track", role: "slider",
                aria_valuemin: "0",
                aria_valuemax: "{state.duration_ms}",
                aria_valuenow: "{state.position_ms}",
                aria_label: "Track position",
                div { class: "seekbar__played", style: "width: {pct}%" }
            }
            span { class: "seekbar__time seekbar__time--total", "{total}" }
        }
    }
}

fn format_time(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_pads_seconds() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(5_000), "0:05");
        assert_eq!(format_time(65_000), "1:05");
        assert_eq!(format_time(125_500), "2:05");
    }
}
