//! Playlists — manual + smart, with M3U8 import/export.
//!
//! - [`manager::PlaylistManager`] wraps the DB queries with helpful
//!   high-level methods (clone, export, import).
//! - [`smart::SmartRules`] is the rules engine for smart playlists.

pub mod manager;
pub mod smart;

pub use manager::{PlaylistManager, M3uExportOptions};
pub use smart::{SmartRules, SmartCondition, SortOrder, evaluate};
