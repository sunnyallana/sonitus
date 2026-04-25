//! Audit logger — JSONL records of every outbound HTTP request.
//!
//! ## What gets logged
//!
//! - Timestamp (ISO 8601 UTC)
//! - Destination host (no path, no query string — those can leak intent)
//! - HTTP method
//! - URL path (with query string stripped of secret-named keys)
//! - Trigger (which user action caused the request)
//! - Bytes sent / received
//! - Status code, duration in ms
//!
//! ## What is NEVER logged
//!
//! - Request bodies
//! - Response bodies
//! - `Authorization` headers
//! - `Cookie` headers
//! - Query string values for keys named `token`, `key`, `secret`,
//!   `password`, `signature`, `code`, or `access_token`.
//!
//! ## Rotation
//!
//! When the active log file exceeds `audit_log_max_mb` (default 5 MiB), it
//! is renamed `audit.log.1`, the previous `.1` becomes `.2`, etc., up to
//! `audit_log_keep_rotations` (default 3). Older rotations are deleted.

use crate::error::{Result, SonitusError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// One audit log record. Serialized as a single JSON object per line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO 8601 UTC timestamp.
    pub ts: DateTime<Utc>,
    /// Destination host (e.g. `www.googleapis.com`). Never the path.
    pub dest: String,
    /// HTTP method (`GET`, `POST`, etc.).
    pub method: String,
    /// URL path. Query string is included only after secret-stripping.
    pub path: String,
    /// What user-facing action triggered this request.
    pub by: TriggerSource,
    /// Bytes transmitted in the request body.
    #[serde(default)]
    pub sent: u64,
    /// Bytes received in the response body.
    #[serde(default)]
    pub recv: u64,
    /// HTTP status code, if the request completed.
    pub status: Option<u16>,
    /// Wall-clock duration in milliseconds.
    pub ms: u64,
    /// Optional error message if the request failed before getting a response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What user-facing action caused this outbound request?
///
/// Lets users see at a glance whether traffic was triggered by something
/// they actively did (a click) or by background activity (a scan, a token
/// refresh). The privacy dashboard groups records by this column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TriggerSource {
    /// A direct user action (clicked play, opened album).
    UserAction,
    /// A scheduled background scan or sync.
    BackgroundScan,
    /// An optional metadata lookup (MusicBrainz / AcoustID).
    MetadataLookup,
    /// An OAuth token refresh.
    OauthRefresh,
    /// A user-initiated download.
    Download,
    /// A streaming playback fetch.
    Playback,
}

/// Append-only logger that writes audit records to disk.
///
/// Thread-safe via an internal `Mutex`. Acquiring the lock is short
/// (one `write_all` + `flush` per entry) so contention is negligible.
pub struct AuditLogger {
    path: PathBuf,
    /// Max size in bytes before rotation.
    max_size: u64,
    /// Number of rotated files to keep.
    keep_rotations: u32,
    inner: Mutex<()>,
}

impl AuditLogger {
    /// Construct a logger writing to `path`. Creates parent directories
    /// and the file itself if absent.
    pub fn new(path: PathBuf, max_size_mb: u64, keep_rotations: u32) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            std::fs::File::create(&path)?;
        }
        Ok(Self {
            path,
            max_size: max_size_mb.saturating_mul(1_024 * 1_024),
            keep_rotations,
            inner: Mutex::new(()),
        })
    }

    /// Append an entry. Rotates the log if it exceeds `max_size`.
    pub fn append(&self, entry: &AuditEntry) -> Result<()> {
        let _guard = self.inner.lock().map_err(|_| {
            SonitusError::AuditWriteFailed("audit logger mutex poisoned".to_string())
        })?;

        // Rotate if size limit exceeded.
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if meta.len() > self.max_size {
                self.rotate_locked()?;
            }
        }

        let line = serde_json::to_string(entry)
            .map_err(|e| SonitusError::AuditWriteFailed(e.to_string()))?;

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
            .map_err(|e| SonitusError::AuditWriteFailed(e.to_string()))?;

        file.write_all(line.as_bytes())
            .map_err(|e| SonitusError::AuditWriteFailed(e.to_string()))?;
        file.write_all(b"\n")
            .map_err(|e| SonitusError::AuditWriteFailed(e.to_string()))?;
        // Flush so a crash doesn't lose the most recent records.
        file.flush()
            .map_err(|e| SonitusError::AuditWriteFailed(e.to_string()))?;
        Ok(())
    }

    /// Rotate the log: `audit.log` → `audit.log.1`, `.1` → `.2`, etc.
    /// Caller holds `self.inner` lock.
    fn rotate_locked(&self) -> Result<()> {
        let base = &self.path;
        // Delete the oldest if it exists.
        let oldest = base.with_extension(format!("log.{}", self.keep_rotations));
        if oldest.exists() {
            std::fs::remove_file(&oldest).ok();
        }
        // Shift down.
        for i in (1..self.keep_rotations).rev() {
            let from = base.with_extension(format!("log.{i}"));
            let to = base.with_extension(format!("log.{}", i + 1));
            if from.exists() {
                std::fs::rename(&from, &to).ok();
            }
        }
        // Move current → .1.
        let first = base.with_extension("log.1");
        if base.exists() {
            std::fs::rename(base, &first).ok();
        }
        // Recreate the empty current file.
        std::fs::File::create(base)?;
        Ok(())
    }

    /// Read all entries from the active log. Used by the privacy
    /// dashboard. Returns an iterator (lazy) so we don't load 5 MiB into
    /// memory unnecessarily.
    pub fn read_entries(&self) -> Result<Vec<AuditEntry>> {
        let _guard = self.inner.lock().map_err(|_| {
            SonitusError::AuditWriteFailed("audit logger mutex poisoned".to_string())
        })?;
        let text = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        for (lineno, raw) in text.lines().enumerate() {
            if raw.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<AuditEntry>(raw) {
                Ok(e) => out.push(e),
                Err(e) => {
                    tracing::warn!(line = lineno + 1, error = %e, "skipped malformed audit line");
                }
            }
        }
        Ok(out)
    }

    /// Stub for `mock-audit` test feature: returns the path being written to.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Strip secret-named query parameters from a URL's query string.
///
/// We log the path (it's useful context for "what did Sonitus do?") but
/// never the *value* of any parameter that smells like a credential.
///
/// Algorithm: parse query as `&`-separated `key=value` pairs, replace any
/// value whose key matches the deny-list with `[REDACTED]`.
pub fn redact_query(path_and_query: &str) -> String {
    let Some((path, query)) = path_and_query.split_once('?') else {
        return path_and_query.to_string();
    };

    let secret_keys = [
        "token", "access_token", "refresh_token", "id_token",
        "key", "api_key", "apikey",
        "secret", "client_secret",
        "password", "passwd", "pwd",
        "signature", "x-amz-signature",
        "code",
    ];

    let redacted: Vec<String> = query
        .split('&')
        .map(|pair| {
            if let Some((k, _v)) = pair.split_once('=') {
                let lk = k.to_ascii_lowercase();
                if secret_keys.iter().any(|s| lk.contains(s)) {
                    format!("{k}=[REDACTED]")
                } else {
                    pair.to_string()
                }
            } else {
                pair.to_string()
            }
        })
        .collect();

    format!("{}?{}", path, redacted.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_logger() -> (TempDir, AuditLogger) {
        let dir = TempDir::new().unwrap();
        let logger = AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap();
        (dir, logger)
    }

    fn fresh_entry() -> AuditEntry {
        AuditEntry {
            ts: Utc::now(),
            dest: "www.googleapis.com".into(),
            method: "GET".into(),
            path: "/drive/v3/files".into(),
            by: TriggerSource::UserAction,
            sent: 312,
            recv: 4821,
            status: Some(200),
            ms: 143,
            error: None,
        }
    }

    #[test]
    fn append_and_read_round_trips_entry() {
        let (_dir, logger) = fresh_logger();
        let e = fresh_entry();
        logger.append(&e).unwrap();
        let back = logger.read_entries().unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].dest, e.dest);
        assert_eq!(back[0].path, e.path);
        assert_eq!(back[0].status, e.status);
    }

    #[test]
    fn append_writes_jsonl_format() {
        let (_dir, logger) = fresh_logger();
        let e = fresh_entry();
        logger.append(&e).unwrap();
        let text = std::fs::read_to_string(logger.path()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        // Single JSON object per line.
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["dest"], "www.googleapis.com");
        assert_eq!(parsed["by"], "user_action");
    }

    #[test]
    fn redact_query_strips_secret_values() {
        let q = redact_query("/oauth/callback?code=4/0aSecret&state=xyz");
        assert!(q.contains("code=[REDACTED]"));
        assert!(q.contains("state=xyz"));
        assert!(!q.contains("4/0aSecret"));
    }

    #[test]
    fn redact_query_handles_no_query_string() {
        assert_eq!(redact_query("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn redact_query_strips_uppercase_keys_too() {
        let q = redact_query("/api?ACCESS_TOKEN=ya29.aSecret&user=alice");
        assert!(q.contains("ACCESS_TOKEN=[REDACTED]"));
        assert!(q.contains("user=alice"));
    }

    #[test]
    fn rotation_creates_numbered_backups_after_size_exceeded() {
        let dir = TempDir::new().unwrap();
        // Tiny limit so a single entry triggers rotation.
        let logger = AuditLogger::new(dir.path().join("audit.log"), 0, 2).unwrap();
        // Manually grow the file beyond limit before next append.
        std::fs::write(logger.path(), b"x".repeat(2_000_000)).unwrap();
        logger.append(&fresh_entry()).unwrap();
        let log_1 = dir.path().join("audit.log.1");
        assert!(log_1.exists(), "rotation should produce audit.log.1");
    }
}
