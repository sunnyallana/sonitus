//! # sonitus-meta
//!
//! User-owned `.sonitus` library file format. This crate has **zero
//! database dependencies** and only `serde` + `toml` + `chrono` for
//! parsing. It compiles for every target Sonitus supports.
//!
//! The `.sonitus` file is the source of truth for the user's library
//! configuration: which sources are connected, which playlists exist,
//! and what privacy preferences are set. The SQLite database is a
//! derived index that can be rebuilt from sources + this file at any time.
//!
//! ## Stability
//!
//! This crate's `LibraryMeta` schema is part of the public file format.
//! Bumping the structural shape without bumping `meta.schema_version`
//! and adding a [`migrate`] step is a breaking change.
//!
//! ## Crash safety
//!
//! [`writer::save`] writes to a temp file in the same directory, fsyncs,
//! then atomically renames over the destination. A power loss can leave
//! either the old file or the new file — never a half-written one.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod migrate;
pub mod reader;
pub mod schema;
pub mod validator;
pub mod writer;

pub use reader::load;
pub use schema::*;
pub use validator::validate;
pub use writer::save;

/// Current `.sonitus` schema version. Increment when the format changes
/// in a way that older versions can't read.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Errors specific to `.sonitus` file handling.
#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    /// File could not be opened/read/written.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// TOML parse failure.
    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),
    /// TOML serialize failure.
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    /// File schema version is newer than this build supports.
    #[error("file schema version {found} is newer than supported version {supported}")]
    UnsupportedVersion {
        /// The version found in the file.
        found: u32,
        /// The maximum supported version.
        supported: u32,
    },
    /// Validation failed.
    #[error("invalid library file: {0}")]
    Invalid(String),
}

/// Result alias for `MetaError`.
pub type MetaResult<T> = std::result::Result<T, MetaError>;
