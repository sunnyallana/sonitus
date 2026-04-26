//! Cover-art component: fetches the album's blob via the orchestrator
//! and renders it as a base64 data URL.
//!
//! Tiny memo cache keyed by album_id avoids re-fetching the same blob
//! when the same album shows up in multiple rows.

use crate::app::use_app_handle;
use base64::Engine;
use dioxus::prelude::*;
use sonitus_core::library::queries;

/// Cover-art tile. Renders a square image if the album has embedded art,
/// otherwise a styled placeholder. `size_class` controls the visual size
/// via CSS (e.g. "cover-art--sm", "cover-art--lg").
#[component]
pub fn CoverArt(album_id: Option<String>, size_class: Option<String>) -> Element {
    let handle = use_app_handle();
    let aid = album_id.clone();
    let blob = use_resource(move || {
        let h = handle.clone();
        let aid = aid.clone();
        async move {
            let h = h?;
            let aid = aid?;
            queries::albums::cover_art_for(h.library.pool(), &aid)
                .await
                .ok()
                .flatten()
        }
    });

    let size = size_class.unwrap_or_else(|| "cover-art--md".to_string());

    let data_url = match &*blob.read_unchecked() {
        Some(Some(bytes)) if !bytes.is_empty() => {
            let mime = sonitus_core::metadata::cover_art::CoverArt::sniff_mime(bytes)
                .unwrap_or("image/jpeg");
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            Some(format!("data:{mime};base64,{b64}"))
        }
        _ => None,
    };

    rsx! {
        div { class: "cover-art {size}",
            match data_url {
                Some(url) => rsx! {
                    img { class: "cover-art__img", src: "{url}", alt: "Cover art" }
                },
                None => rsx! {
                    div { class: "cover-art__placeholder", "♪" }
                }
            }
        }
    }
}
