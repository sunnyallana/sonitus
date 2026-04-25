//! `use_library()` — read library state.

use crate::state::library_state::LibraryState;
use dioxus::prelude::*;

/// Handle returned by [`use_library`].
#[derive(Clone, Copy)]
pub struct LibraryHandle {
    state: Signal<LibraryState>,
}

impl LibraryHandle {
    /// Snapshot of the library state.
    pub fn read(&self) -> LibraryState {
        self.state.read().clone()
    }

    /// True if any source is currently scanning.
    pub fn is_any_scanning(&self) -> bool {
        !self.state.read().scan_progress.is_empty()
    }
}

/// Hook to access library state.
pub fn use_library() -> LibraryHandle {
    let state = use_context::<Signal<LibraryState>>();
    LibraryHandle { state }
}
