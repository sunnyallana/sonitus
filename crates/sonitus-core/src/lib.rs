//! # sonitus-core
//!
//! The core engine of Sonitus — a local-first, encrypted music streaming
//! library. This crate has **zero UI dependencies** and compiles to every
//! platform Sonitus supports, including `wasm32-unknown-unknown`.
//!
//! The crate is organized into independent layers that communicate via
//! channels and traits:
//!
//! - [`crypto`] — Argon2id KDF, XChaCha20-Poly1305 field encryption,
//!   `Zeroize`-aware secret types. Everything privacy-relevant lives here.
//! - [`privacy`] — `AuditMiddleware` for `reqwest`, `ConsentStore` for
//!   opt-in features, `tracing` redact layer.
//! - [`metadata`] — Tag parsing (ID3, FLAC, OGG), Symphonia probe,
//!   cover-art extraction, optional MusicBrainz/AcoustID lookups.
//! - [`library`] — SQLite-backed library: models, queries, FTS5 search,
//!   recursive scanner, live `notify` watcher.
//! - [`player`] — Symphonia decode loop on a dedicated OS thread, cpal
//!   output, gapless pre-buffering, EBU R128 ReplayGain.
//! - [`sources`] — Pluggable `SourceProvider` trait + implementations for
//!   local, Google Drive, S3, SMB, HTTP, Dropbox, OneDrive.
//! - [`download`] — Resumable download queue, BLAKE3-keyed offline cache.
//! - [`playlist`] — Playlist CRUD, smart-playlist rules engine, M3U8 import/export.
//!
//! ## Privacy guarantees enforced by this crate
//!
//! - **No raw `reqwest::Client`** is ever exposed. All HTTP goes through
//!   [`privacy::http_client()`] which wraps every call in [`AuditMiddleware`].
//! - **No plaintext secret** ever crosses the FFI boundary or hits disk.
//!   See [`crypto::Secret`] and [`crypto::field`].
//! - **No telemetry**. The crate has no analytics dependencies; `cargo deny`
//!   blocks them at build time.
//!
//! [`AuditMiddleware`]: privacy::middleware::AuditMiddleware

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

pub mod config;
pub mod crypto;
pub mod download;
pub mod error;
pub mod library;
pub mod metadata;
pub mod player;
pub mod playlist;
pub mod privacy;
pub mod sources;

// Re-exports for the most commonly used public surface.
pub use config::AppConfig;
pub use error::{Result, SonitusError};

/// The version string this crate was built with.
///
/// Used by the UI's "About" page and the `User-Agent` header on outbound
/// HTTP requests (so audited destinations can attribute the request).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The HTTP `User-Agent` Sonitus uses on every outbound request.
///
/// Format: `Sonitus/<version> (+https://sonitus.app)`.
/// We deliberately do NOT include OS, architecture, or any other
/// fingerprintable detail — that would compromise the privacy guarantee.
pub const USER_AGENT: &str = concat!("Sonitus/", env!("CARGO_PKG_VERSION"), " (+https://sonitus.app)");

/// Initialize logging for the crate.
///
/// Installs the `tracing-subscriber` with:
/// - JSON formatter (suitable for piping to disk or stdout)
/// - The redaction layer from [`privacy::redact`]
/// - `RUST_LOG`-driven filter (defaults to `info`)
///
/// Idempotent — safe to call multiple times.
pub fn init_logging() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sonitus=debug"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(privacy::redact::RedactLayer::new())
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .try_init();
}
