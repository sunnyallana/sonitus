//! Download manager and offline cache.
//!
//! - [`manager::DownloadManager`] — concurrent queue with resume.
//! - [`cache::OfflineCache`] — BLAKE3-keyed LRU on disk.

pub mod cache;
pub mod manager;

pub use cache::OfflineCache;
pub use manager::DownloadManager;
