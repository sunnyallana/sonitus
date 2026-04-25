//! Dropbox source — Dropbox API v2.
//!
//! Auth via OAuth2. Listing uses `/2/files/list_folder` (recursive); the
//! response is paginated via `cursor` + `has_more`. Streaming uses
//! `/2/files/download` with the special `Dropbox-API-Arg` header
//! containing JSON `{"path":"/abs/path"}`.

use crate::crypto::Secret;
use crate::error::{Result, SonitusError};
use crate::library::models::{SourceKind, TrackFormat};
use crate::privacy::{AuditLogger, TriggerSource, http_client};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;

const API: &str = "https://api.dropboxapi.com";
const CONTENT: &str = "https://content.dropboxapi.com";

/// Dropbox source.
pub struct DropboxSource {
    id: String,
    name: String,
    audit: Arc<AuditLogger>,
    tokens: RwLock<Tokens>,
    client_id: String,
    client_secret: Secret<String>,
}

#[derive(Debug)]
struct Tokens {
    access: Secret<String>,
    refresh: Option<Secret<String>>,
    expires_at: Option<i64>,
}

#[derive(Serialize)]
struct ListFolderArg<'a> {
    path: &'a str,
    recursive: bool,
    include_media_info: bool,
}

#[derive(Serialize)]
struct ContinueArg<'a> {
    cursor: &'a str,
}

#[derive(Deserialize)]
struct ListFolderResponse {
    entries: Vec<DropboxEntry>,
    cursor: String,
    has_more: bool,
}

#[derive(Deserialize)]
struct DropboxEntry {
    #[serde(rename = ".tag")]
    tag: String,
    path_display: Option<String>,
    size: Option<u64>,
    server_modified: Option<String>,
}

#[derive(Serialize)]
struct DownloadArg<'a> {
    path: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

impl DropboxSource {
    /// Construct given existing creds.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
        client_id: String,
        client_secret: String,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            audit,
            tokens: RwLock::new(Tokens {
                access: Secret::new(access_token),
                refresh: refresh_token.map(Secret::new),
                expires_at,
            }),
            client_id,
            client_secret: Secret::new(client_secret),
        }
    }

    async fn ensure_fresh(&self) -> Result<()> {
        let needs = {
            let r = self.tokens.read();
            r.expires_at
                .map(|e| chrono::Utc::now().timestamp() + 60 > e)
                .unwrap_or(false)
        };
        if needs {
            self.refresh_token().await?;
        }
        Ok(())
    }

    async fn refresh_token(&self) -> Result<()> {
        let refresh = {
            let r = self.tokens.read();
            r.refresh.as_ref().map(|t| t.expose().clone())
        };
        let Some(refresh) = refresh else {
            return Err(SonitusError::NotAuthenticated(self.id.clone()));
        };
        let client = http_client(self.audit.clone(), TriggerSource::OauthRefresh)?;
        let resp = client
            .post(format!("{API}/oauth2/token"))
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.expose().as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|e| SonitusError::OAuth(e.to_string()))?
            .error_for_status()
            .map_err(|e| SonitusError::OAuth(e.to_string()))?;
        let tok: TokenResponse = resp
            .json()
            .await
            .map_err(|e| SonitusError::OAuth(e.to_string()))?;
        let mut w = self.tokens.write();
        w.access = Secret::new(tok.access_token);
        if let Some(r) = tok.refresh_token { w.refresh = Some(Secret::new(r)); }
        if let Some(e) = tok.expires_in {
            w.expires_at = Some(chrono::Utc::now().timestamp() + e);
        }
        Ok(())
    }

    fn access(&self) -> String { self.tokens.read().access.expose().clone() }
}

#[async_trait]
impl SourceProvider for DropboxSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::Dropbox }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::UserAction)?;
        let resp = client
            .post(format!("{API}/2/users/get_current_account"))
            .bearer_auth(self.access())
            .header("Content-Length", "0")
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        if !resp.status().is_success() {
            return Err(SonitusError::HttpStatus {
                status: resp.status().as_u16(),
                message: "dropbox ping failed".into(),
            });
        }
        Ok(())
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;

        let mut out = Vec::new();
        let mut resp: ListFolderResponse = client
            .post(format!("{API}/2/files/list_folder"))
            .bearer_auth(self.access())
            .json(&ListFolderArg { path: "", recursive: true, include_media_info: false })
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
            .json()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;

        loop {
            for entry in &resp.entries {
                if entry.tag != "file" { continue; }
                let Some(path) = &entry.path_display else { continue; };
                let ext = path.rsplit('.').next().map(|s| s.to_ascii_lowercase());
                if !ext.as_deref().and_then(TrackFormat::from_extension).is_some() { continue; }
                out.push(RemoteFile {
                    path: path.clone(),
                    size_bytes: entry.size.unwrap_or(0),
                    modified_at: entry.server_modified.as_deref().and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
                    }),
                    mime_hint: None,
                });
            }

            if !resp.has_more { break; }

            resp = client
                .post(format!("{API}/2/files/list_folder/continue"))
                .bearer_auth(self.access())
                .json(&ContinueArg { cursor: &resp.cursor })
                .send()
                .await
                .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
                .error_for_status()
                .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
                .json()
                .await
                .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        }
        Ok(out)
    }

    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::Playback)?;
        let arg = serde_json::to_string(&DownloadArg { path })
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        let mut req = client
            .post(format!("{CONTENT}/2/files/download"))
            .bearer_auth(self.access())
            .header("Dropbox-API-Arg", arg);
        if let Some(r) = range {
            req = req.header("Range", format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        let stream = resp.bytes_stream();
        let reader = tokio_util::io::StreamReader::new(
            stream.map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))),
        );
        Ok(Box::new(reader))
    }

    async fn download(
        &self,
        path: &str,
        dest: &Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::Download)?;
        let arg = serde_json::to_string(&DownloadArg { path })
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        let resume_from = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
        let mut req = client
            .post(format!("{CONTENT}/2/files/download"))
            .bearer_auth(self.access())
            .header("Dropbox-API-Arg", arg);
        if resume_from > 0 {
            req = req.header("Range", format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        let total = resp.content_length().map(|c| c + resume_from);
        if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
        let mut out = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await?;
        let mut s = resp.bytes_stream();
        let mut done = resume_from;
        let dl = std::time::Instant::now();
        while let Some(chunk) = s.next().await {
            let chunk = chunk.map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
            tokio::io::AsyncWriteExt::write_all(&mut out, &chunk).await?;
            done += chunk.len() as u64;
            let elapsed = dl.elapsed().as_secs_f64().max(0.001);
            let _ = progress.send(DownloadProgress {
                bytes_done: done,
                bytes_total: total,
                speed_bps: Some(((done - resume_from) as f64 / elapsed) as u64),
            }).await;
        }
        Ok(())
    }

    async fn read_bytes(&self, path: &str, max_bytes: usize) -> Result<Bytes> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;
        let arg = serde_json::to_string(&DownloadArg { path })
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        let resp = client
            .post(format!("{CONTENT}/2/files/download"))
            .bearer_auth(self.access())
            .header("Dropbox-API-Arg", arg)
            .header("Range", format!("bytes=0-{}", max_bytes.saturating_sub(1)))
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?;
        Ok(resp.bytes().await
            .map_err(|e| SonitusError::Source { kind: "dropbox", message: e.to_string() })?)
    }
}
