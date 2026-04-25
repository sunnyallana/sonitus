//! Root layout: sidebar + topbar + content + now-playing bar.
//!
//! On mobile (narrow viewport), sidebar collapses into a bottom nav.
//! On the web, the same layout is responsive across breakpoints.

use crate::components::layout::{
    bottom_nav::BottomNav, now_playing_bar::NowPlayingBar, sidebar::Sidebar, topbar::Topbar,
};
use dioxus::prelude::*;

/// Wrapping layout component used by every routed page (except `/now-playing`).
#[component]
pub fn AppShell() -> Element {
    rsx! {
        div { class: "app-shell",
            div { class: "app-shell__main",
                Sidebar {}
                div { class: "app-shell__content",
                    Topbar {}
                    main { class: "app-shell__page",
                        Outlet::<crate::routes::Route> {}
                    }
                }
            }
            NowPlayingBar {}
            BottomNav {}
        }
    }
}
