//! Single-table query modules.
//!
//! Each submodule wraps the CRUD operations for one table. We deliberately
//! split these out so the cognitive surface stays small; the caller picks
//! which module they need:
//!
//! ```ignore
//! use sonitus_core::library::{Library, queries};
//!
//! let lib: Library = ...;
//! let track = queries::tracks::by_id(lib.pool(), "track-id").await?;
//! ```

pub mod albums;
pub mod artists;
pub mod downloads;
pub mod playlists;
pub mod sources;
pub mod tracks;
