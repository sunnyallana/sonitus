//! Crate-wide error type and `Result` alias.
//!
//! Every fallible function in `sonitus-core` returns [`Result<T>`]. We use
//! a single error enum so callers (the UI, mainly) can match on a closed set
//! of variants and present localized error messages.

use std::path::PathBuf;
use thiserror::Error;

/// Convenience alias for `Result<T, SonitusError>`.
pub type Result<T> = std::result::Result<T, SonitusError>;

/// All error conditions that can be produced by the Sonitus core.
///
/// Each variant carries enough context for the UI to render a meaningful
/// message without having to look up a side channel. **Never** include a
/// secret value (token, passphrase, key) in an error message.
#[derive(Error, Debug)]
pub enum SonitusError {
    // ─── Configuration / startup ─────────────────────────────────────────
    /// The application configuration on disk could not be parsed.
    #[error("invalid configuration: {0}")]
    ConfigInvalid(String),

    /// A required configuration directory could not be located.
    /// On Linux this is `$XDG_CONFIG_HOME` or `~/.config`; on macOS it is
    /// `~/Library/Application Support`; on Windows it is `%APPDATA%`.
    #[error("could not determine platform config directory")]
    NoConfigDir,

    // ─── Cryptography ────────────────────────────────────────────────────
    /// Argon2id key derivation failed (typically: invalid parameters).
    #[error("key derivation failed: {0}")]
    KdfFailed(String),

    /// AEAD encryption or decryption failed.
    /// On decrypt this almost always means the wrong key was used or the
    /// ciphertext was corrupted — both are unrecoverable.
    #[error("crypto error: {0}")]
    Crypto(&'static str),

    /// The input to a crypto routine was the wrong length (e.g. nonce).
    #[error("crypto input too short: needed {needed} bytes, got {got}")]
    CryptoTooShort {
        /// The minimum required length in bytes.
        needed: usize,
        /// The actual length in bytes.
        got: usize,
    },

    // ─── Database ────────────────────────────────────────────────────────
    /// An sqlx-level database error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// A SQL migration failed to apply.
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// A row that should exist was not found by ID.
    #[error("{kind} not found: {id}")]
    NotFound {
        /// The kind of entity (`"track"`, `"album"`, etc.).
        kind: &'static str,
        /// The identifier searched for.
        id: String,
    },

    // ─── HTTP / network ──────────────────────────────────────────────────
    /// A `reqwest` HTTP error, including timeouts and DNS failures.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// A `reqwest_middleware` chain error.
    #[error("HTTP middleware error: {0}")]
    HttpMiddleware(String),

    /// The server returned a non-success status code.
    #[error("HTTP {status}: {message}")]
    HttpStatus {
        /// The HTTP status code returned.
        status: u16,
        /// A short message explaining the failure (no body content).
        message: String,
    },

    // ─── Source provider ─────────────────────────────────────────────────
    /// A source provider operation failed (kind-specific message).
    #[error("source provider error ({kind}): {message}")]
    Source {
        /// The kind of source — `"local"`, `"google_drive"`, etc.
        kind: &'static str,
        /// The error message.
        message: String,
    },

    /// The user has not yet completed the OAuth flow for this source.
    #[error("source {0} is not authenticated")]
    NotAuthenticated(String),

    // ─── Audio / player ──────────────────────────────────────────────────
    /// Symphonia could not probe or decode the audio file.
    #[error("audio decode error: {0}")]
    Audio(String),

    /// The cpal audio output backend reported an error.
    #[error("audio output error: {0}")]
    AudioOutput(String),

    /// The requested codec or container is not supported.
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),

    // ─── Filesystem / IO ─────────────────────────────────────────────────
    /// A standard IO error (read/write/seek/etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A path that should exist did not.
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    /// A file watcher could not be created.
    #[error("notify watcher error: {0}")]
    Notify(#[from] notify::Error),

    // ─── Serialization ───────────────────────────────────────────────────
    /// JSON serialize/deserialize failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML serialize failure.
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// TOML deserialize failure.
    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// `.sonitus` metadata file error (delegated to the meta crate).
    #[error("metadata file error: {0}")]
    Meta(String),

    // ─── Privacy / consent ───────────────────────────────────────────────
    /// A feature was used before the user consented to it.
    #[error("privacy consent required for: {feature}")]
    ConsentRequired {
        /// The feature name (matches the `Feature` enum).
        feature: &'static str,
    },

    /// The audit log could not be written. Operations that depend on the
    /// audit log (i.e. all outbound HTTP) MUST fail in this case rather
    /// than silently proceeding without an audit record.
    #[error("audit log write failed: {0}")]
    AuditWriteFailed(String),

    // ─── OAuth ───────────────────────────────────────────────────────────
    /// The OAuth flow failed (e.g. user denied, invalid code, refresh failed).
    #[error("OAuth error: {0}")]
    OAuth(String),

    // ─── Catch-all ───────────────────────────────────────────────────────
    /// An unexpected error wrapped via `anyhow`.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl SonitusError {
    /// True if this error is likely transient and worth retrying.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Http(e) => e.is_timeout() || e.is_connect(),
            Self::HttpStatus { status, .. } => matches!(*status, 500..=599 | 408 | 429),
            Self::Io(e) => matches!(e.kind(),
                std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::WouldBlock),
            _ => false,
        }
    }
}
