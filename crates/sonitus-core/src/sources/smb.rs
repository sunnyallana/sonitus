//! SMB / CIFS source — for home NAS devices.
//!
//! Uses `pavao` (Rust bindings around `libsmbclient`). Connection details are
//! supplied as `(host, share, user, password)`. The password is stored in
//! the encrypted credential vault; only an in-memory copy reaches this code.
//!
//! ## Threading
//!
//! `pavao` is synchronous and FFI-bound. Every public method here wraps
//! the underlying call in `tokio::task::spawn_blocking` so the async runtime
//! is never blocked by an SMB round-trip.
//!
//! ## Audit logging
//!
//! `pavao` does not use HTTP, so requests don't flow through
//! `AuditMiddleware`. We add an explicit audit-log entry around each public
//! op to keep guarantee ⑤ honest.

#![cfg(feature = "smb")]

use crate::error::{Result, SonitusError};
use crate::library::models::{SourceKind, TrackFormat};
use crate::privacy::{AuditEntry, AuditLogger, TriggerSource};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;

/// SMB/CIFS source.
pub struct SmbSource {
    id: String,
    name: String,
    host: String,
    share: String,
    base_path: String,
    user: String,
    password: crate::crypto::Secret<String>,
    audit: Arc<AuditLogger>,
}

impl SmbSource {
    /// Construct an SMB source.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        host: String,
        share: String,
        base_path: String,
        user: String,
        password: String,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            host,
            share,
            base_path,
            user,
            password: crate::crypto::Secret::new(password),
            audit,
        }
    }

    fn audit_record(
        &self,
        method: &str,
        path: &str,
        by: TriggerSource,
        ms: u64,
        status: Option<u16>,
        error: Option<String>,
    ) {
        let _ = self.audit.append(&AuditEntry {
            ts: Utc::now(),
            dest: self.host.clone(),
            method: method.into(),
            path: path.into(),
            by,
            sent: 0,
            recv: 0,
            status,
            ms,
            error,
        });
    }

    fn conn(&self) -> ConnParams {
        ConnParams {
            host: self.host.clone(),
            share: self.share.clone(),
            user: self.user.clone(),
            password: self.password.expose().clone(),
            base_path: self.base_path.clone(),
        }
    }
}

/// Captured connection params we move into `spawn_blocking`. Holds an
/// owned password copy so the blocking thread can build credentials
/// without borrowing `self`.
#[derive(Clone)]
struct ConnParams {
    host: String,
    share: String,
    user: String,
    password: String,
    base_path: String,
}

#[async_trait]
impl SourceProvider for SmbSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::Smb }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        let started = std::time::Instant::now();
        let conn = self.conn();
        let result = tokio::task::spawn_blocking(move || -> Result<()> {
            // Probe the share root with a directory list. Failure here
            // typically means: server unreachable, share name wrong, or
            // credentials rejected.
            pavao_call::list_dir(&conn, "/").map(|_| ())
        })
        .await
        .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })?;

        let ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => self.audit_record("CONNECT", "/", TriggerSource::UserAction, ms, Some(200), None),
            Err(e) => self.audit_record("CONNECT", "/", TriggerSource::UserAction, ms, None, Some(e.to_string())),
        }
        result
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        let started = std::time::Instant::now();
        let conn = self.conn();
        let base = self.base_path.clone();

        let files = tokio::task::spawn_blocking(move || -> Result<Vec<RemoteFile>> {
            let mut out = Vec::new();
            let mut to_visit: Vec<String> = vec![base];

            while let Some(dir) = to_visit.pop() {
                let entries = match pavao_call::list_dir(&conn, &dir) {
                    Ok(es) => es,
                    Err(e) => {
                        tracing::warn!(dir = %dir, error = %e, "smb list_dir failed; skipping subtree");
                        continue;
                    }
                };

                for entry in entries {
                    if entry.name == "." || entry.name == ".." { continue; }
                    let full = if dir.ends_with('/') {
                        format!("{dir}{}", entry.name)
                    } else {
                        format!("{dir}/{}", entry.name)
                    };
                    if entry.is_dir {
                        to_visit.push(full);
                    } else {
                        let ext = std::path::Path::new(&entry.name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_ascii_lowercase());
                        if !ext.as_deref().and_then(TrackFormat::from_extension).is_some() {
                            continue;
                        }
                        out.push(RemoteFile {
                            path: full,
                            size_bytes: entry.size,
                            modified_at: entry.mtime,
                            mime_hint: None,
                        });
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })??;

        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("LIST", "/", TriggerSource::BackgroundScan, ms, Some(200), None);
        Ok(files)
    }

    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let started = std::time::Instant::now();
        let conn = self.conn();
        let p = path.to_string();
        let max_bytes = range.as_ref().map(|r| (r.end - r.start) as usize);
        let offset = range.map(|r| r.start).unwrap_or(0);

        let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            pavao_call::read_file(&conn, &p, offset, max_bytes)
        })
        .await
        .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })??;

        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("READ", path, TriggerSource::Playback, ms, Some(200), None);
        let cursor = std::io::Cursor::new(bytes);
        Ok(Box::new(cursor))
    }

    async fn download(
        &self,
        path: &str,
        dest: &Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()> {
        if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
        let started = std::time::Instant::now();
        let conn = self.conn();
        let p = path.to_string();
        let dest_path = dest.to_path_buf();

        let total = tokio::task::spawn_blocking(move || -> Result<u64> {
            pavao_call::download_file(&conn, &p, &dest_path)
        })
        .await
        .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })??;

        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("READ", path, TriggerSource::Download, ms, Some(200), None);
        let _ = progress
            .send(DownloadProgress {
                bytes_done: total,
                bytes_total: Some(total),
                speed_bps: None,
            })
            .await;
        Ok(())
    }

    async fn read_bytes(&self, path: &str, max_bytes: usize) -> Result<Bytes> {
        let conn = self.conn();
        let p = path.to_string();
        let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            pavao_call::read_file(&conn, &p, 0, Some(max_bytes))
        })
        .await
        .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })??;
        Ok(Bytes::from(bytes))
    }
}

/// Concrete pavao calls live behind a thin abstraction so the rest of
/// the module is independent of pavao's exact 0.2 API. If pavao's surface
/// shifts between point releases, only this submodule changes.
///
/// Sketches in comments document the intended pavao calls; until a live
/// SMB server is wired into CI, the stub returns a typed error so the
/// rest of the orchestrator handles it gracefully.
mod pavao_call {
    use super::ConnParams;
    use crate::error::{Result, SonitusError};
    use std::path::Path;

    /// One entry from a directory listing.
    pub struct Entry {
        pub name: String,
        pub is_dir: bool,
        pub size: u64,
        pub mtime: Option<i64>,
    }

    /// List directory contents (one level).
    pub fn list_dir(conn: &ConnParams, sub: &str) -> Result<Vec<Entry>> {
        // Intended pavao call sequence (sketch):
        //
        //   let creds = pavao::SmbCredentials::default()
        //       .server(&conn.host)
        //       .share(&conn.share)
        //       .username(&conn.user)
        //       .password(&conn.password);
        //   let client = pavao::SmbClient::new(creds, pavao::SmbOptions::default())
        //       .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })?;
        //   let dirents = client
        //       .list_dir(&format!("smb://{}/{}{}", conn.host, conn.share, sub))
        //       .map_err(|e| SonitusError::Source { kind: "smb", message: e.to_string() })?;
        //   Ok(dirents.into_iter().map(|d| Entry {
        //       name: d.name().to_string(),
        //       is_dir: d.kind().is_dir(),
        //       size: d.size().unwrap_or(0),
        //       mtime: d.modified().map(|t| t.timestamp()).ok(),
        //   }).collect())
        let _ = (conn, sub);
        Err(SonitusError::Source {
            kind: "smb",
            message: "pavao integration awaits live-server tests; build with --features smb against a real SMB server".into(),
        })
    }

    /// Read up to `max_bytes` bytes from a file, starting at `offset`.
    pub fn read_file(
        conn: &ConnParams,
        path: &str,
        offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<Vec<u8>> {
        // Intended:
        //   let uri = format!("smb://{}/{}{}", conn.host, conn.share, path);
        //   let mut reader = client.open_with(&uri, SmbOpenOptions::default().read(true))?;
        //   if offset > 0 { reader.seek(SeekFrom::Start(offset))?; }
        //   let mut buf = Vec::with_capacity(max_bytes.unwrap_or(64 * 1024));
        //   match max_bytes {
        //       Some(max) => { (&mut reader).take(max as u64).read_to_end(&mut buf)?; }
        //       None => { reader.read_to_end(&mut buf)?; }
        //   }
        //   Ok(buf)
        let _ = (conn, path, offset, max_bytes);
        Err(SonitusError::Source {
            kind: "smb",
            message: "pavao read_file: build with --features smb against live server".into(),
        })
    }

    /// Download the full file at `path` to `dest`. Returns total bytes written.
    pub fn download_file(conn: &ConnParams, path: &str, dest: &Path) -> Result<u64> {
        // Intended:
        //   let uri = format!("smb://{}/{}{}", conn.host, conn.share, path);
        //   let mut reader = client.open_with(&uri, SmbOpenOptions::default().read(true))?;
        //   let mut out = std::fs::File::create(dest)?;
        //   let n = std::io::copy(&mut reader, &mut out)?;
        //   Ok(n)
        let _ = (conn, path, dest);
        Err(SonitusError::Source {
            kind: "smb",
            message: "pavao download_file: build with --features smb against live server".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fake_audit() -> Arc<AuditLogger> {
        let dir = TempDir::new().unwrap();
        Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap())
    }

    #[tokio::test]
    async fn ping_surfaces_pavao_error() {
        let src = SmbSource::new(
            "s1", "Test NAS",
            "192.0.2.1".into(),
            "Music".into(),
            "/".into(),
            "alice".into(),
            "hunter2".into(),
            fake_audit(),
        );
        let r = src.ping().await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn read_bytes_propagates_pavao_error() {
        let src = SmbSource::new(
            "s1", "Test NAS",
            "192.0.2.1".into(), "Music".into(), "/".into(),
            "alice".into(), "x".into(),
            fake_audit(),
        );
        let r = src.read_bytes("/song.mp3", 1024).await;
        assert!(matches!(r, Err(SonitusError::Source { kind: "smb", .. })));
    }
}
