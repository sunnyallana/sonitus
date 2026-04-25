//! The library — the user's music collection, indexed and queryable.
//!
//! The library is built from sources (local folders, cloud drives) by the
//! [`scanner`]. Once indexed, tracks are queryable via the [`queries`]
//! module and full-text searchable via [`search`]. A [`watcher`] keeps
//! the index live as filesystem events arrive.
//!
//! ## Data flow
//!
//! ```text
//!     Source                  Scanner                    SQLite VaultDb
//!  ┌─────────┐  list_files  ┌────────┐    upsert       ┌──────────────┐
//!  │ local   ├──────────────► scan() ├──────────────────► tracks       │
//!  │ drive   │              │        │                  │ artists      │
//!  │ s3      │              └────────┘                  │ albums       │
//!  │ ...     │                                          │ tracks_fts   │
//!  └─────────┘                                          └──────────────┘
//!         ▲                                                      │
//!         │                                                      │
//!         │            ┌─────────┐    queries::*                 │
//!         └────────────┤ watcher │◄──────────────────────────────┘
//!                      └─────────┘
//! ```

pub mod db;
pub mod models;
pub mod queries;
pub mod scanner;
pub mod search;
pub mod watcher;

pub use db::Library;
pub use models::{Album, Artist, Playlist, ScanState, Source, SourceKind, Track, TrackFormat};
pub use scanner::{ScanProgress, ScanReport, Scanner};
pub use search::{SearchKind, SearchResult, search};
pub use watcher::LibraryWatcher;
