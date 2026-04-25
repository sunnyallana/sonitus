//! Scrub bar showing playback progress.
//!
//! Implemented as `<input type="range">` so the browser handles all the
//! click/drag arithmetic correctly — no element-size guessing required.

use crate::app::use_app_handle;
use crate::hooks::use_player::use_player;
use dioxus::prelude::*;

/// Seekbar — click or drag to seek; visual fill follows playback position.
#[component]
pub fn Seekbar() -> Element {
    let player = use_player();
    let state = player.read();
    let cur = format_time(state.position_ms);
    let total = format_time(state.duration_ms);
    let max = state.duration_ms.max(1);
    let value = state.position_ms.min(max);
    // Width of the played-portion overlay, matching the slider's value.
    let pct = if max > 0 {
        (value as f64 / max as f64) * 100.0
    } else {
        0.0
    };

    let handle = use_app_handle();
    let on_input = move |evt: FormEvent| {
        let Some(handle) = handle.clone() else { return; };
        let Ok(ms) = evt.value().parse::<u64>() else { return; };
        handle.seek(ms as f64 / 1000.0);
    };

    rsx! {
        div { class: "seekbar",
            span { class: "seekbar__time seekbar__time--cur", "{cur}" }
            div { class: "seekbar__bar",
                // Visual overlay showing played fraction. The actual slider
                // is on top; CSS makes it transparent so the overlay shows
                // through.
                div { class: "seekbar__played", style: "width: {pct}%" }
                input {
                    class: "seekbar__slider",
                    r#type: "range",
                    min: "0",
                    max: "{max}",
                    value: "{value}",
                    aria_label: "Track position",
                    oninput: on_input,
                }
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
