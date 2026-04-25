//! `use_search()` — read search state.

use crate::state::search_state::SearchState;
use dioxus::prelude::*;

/// Handle returned by [`use_search`].
#[derive(Clone, Copy)]
pub struct SearchHandle {
    state: Signal<SearchState>,
}

impl SearchHandle {
    /// Snapshot of the search state.
    pub fn read(&self) -> SearchState {
        self.state.read().clone()
    }

    /// Update the query (and trigger debounced execution elsewhere).
    pub fn set_query(&self, query: String) {
        self.state.clone().write().query = query;
    }
}

/// Hook to access search state.
pub fn use_search() -> SearchHandle {
    let state = use_context::<Signal<SearchState>>();
    SearchHandle { state }
}
