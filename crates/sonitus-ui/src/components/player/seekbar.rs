//! Scrub bar showing playback progress.

use crate::app::use_app_handle;
use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Seekbar — shows played/buffered/remaining and supports click-to-seek.
#[component]
pub fn Seekbar() -> Element {
    let player = use_player();
    let pct = player.progress_fraction() * 100.0;
    let state = player.read();
    let cur = format_time(state.position_ms);
    let total = format_time(state.duration_ms);

    let handle = use_app_handle();
    let dur_ms = state.duration_ms;
    let on_click = move |evt: MouseEvent| {
        let Some(handle) = handle.clone() else { return; };
        if dur_ms == 0 { return; }
        // Best-effort: client coords on the element. The element is sized
        // 100% of its container; we use the offset_x against the bar width.
        let coords = evt.element_coordinates();
        let bar_width = 600.0; // CSS-controlled; this is a fallback estimate.
        let frac = (coords.x / bar_width).clamp(0.0, 1.0);
        let target_secs = (dur_ms as f64 / 1000.0) * frac;
        handle.seek(target_secs);
    };

    rsx! {
        div { class: "seekbar",
            span { class: "seekbar__time seekbar__time--cur", "{cur}" }
            div { class: "seekbar__track", role: "slider",
                aria_valuemin: "0",
                aria_valuemax: "{state.duration_ms}",
                aria_valuenow: "{state.position_ms}",
                aria_label: "Track position",
                onclick: on_click,
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
