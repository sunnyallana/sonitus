//! Search state — current query, results, loading flag.

use dioxus::prelude::*;
use sonitus_core::library::SearchResult;

/// Search state shared across the search bar + results page.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// Current query (debounced).
    pub query: String,
    /// Results from the most recent successful search.
    pub results: Vec<SearchResult>,
    /// True while a search is in flight.
    pub loading: bool,
    /// Last error, if a search failed.
    pub error: Option<String>,
}

/// Install a `Signal<SearchState>` into the context.
pub fn install_search_state() {
    use_context_provider(|| Signal::new(SearchState::default()));
}
