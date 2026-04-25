//! Root component — installs all global context and the router.

use crate::routes::Route;
use crate::state::{
    download_state::install_download_state,
    library_state::install_library_state,
    player_state::install_player_state,
    search_state::install_search_state,
    settings_state::install_settings_state,
};
use dioxus::prelude::*;

const STYLE_CSS: Asset = asset!("/assets/styles/app.css");
const FONT_INTER: Asset = asset!("/assets/fonts/Inter-Variable.woff2");

/// Top-level app component. Mounts `Router` plus global Signals.
#[component]
pub fn App() -> Element {
    // Install global state into the Dioxus context. Order matters: settings
    // and player_state are queried by other contexts when constructing.
    install_settings_state();
    install_library_state();
    install_player_state();
    install_download_state();
    install_search_state();

    rsx! {
        document::Stylesheet { href: STYLE_CSS }
        document::Link { rel: "preload", as_: "font", href: FONT_INTER, crossorigin: "anonymous" }
        document::Title { "Sonitus" }
        document::Meta { name: "color-scheme", content: "dark light" }

        Router::<Route> {}
    }
}
