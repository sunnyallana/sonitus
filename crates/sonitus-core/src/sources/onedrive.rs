//! Microsoft OneDrive source — Microsoft Graph API.
//!
//! Auth via OAuth2 (Azure AD). Listing uses `/me/drive/root/children`
//! recursively. Streaming uses `/me/drive/items/{id}/content` with
//! standard `Range:` headers.

use crate::crypto::Secret;
use crate::error::{Result, SonitusError};
use crate::library::models::{SourceKind, TrackFormat};
use crate::privacy::{AuditLogger, TriggerSource, http_client};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use parking_lot::RwLock;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;

const GRAPH: &str = "https://graph.microsoft.com/v1.0";

/// OneDrive source.
pub struct OneDriveSource {
    id: String,
    name: String,
    audit: Arc<AuditLogger>,
    tokens: RwLock<Tokens>,
    client_id: String,
    client_secret: Secret<String>,
    /// Tenant: "common" for personal accounts, GUID for orgs.
    tenant: String,
}

#[derive(Debug)]
struct Tokens {
    access: Secret<String>,
    refresh: Option<Secret<String>>,
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct GraphChildrenResponse {
    value: Vec<GraphItem>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct GraphItem {
    id: String,
    name: String,
    size: Option<u64>,
    folder: Option<serde_json::Value>,
    file: Option<GraphFile>,
    #[serde(rename = "lastModifiedDateTime")]
    last_modified: Option<String>,
    #[serde(rename = "parentReference")]
    parent_reference: Option<GraphParent>,
}

#[derive(Deserialize)]
struct GraphFile {
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

#[derive(Deserialize)]
struct GraphParent {
    path: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

impl OneDriveSource {
    /// Construct given existing creds.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
        client_id: String,
        client_secret: String,
        tenant: String,
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
            tenant,
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
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant
        );
        let resp = client
            .post(url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.expose().as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
                ("scope", "Files.Read offline_access"),
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
impl SourceProvider for OneDriveSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::Onedrive }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::UserAction)?;
        let resp = client
            .get(format!("{GRAPH}/me/drive"))
            .bearer_auth(self.access())
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;
        if !resp.status().is_success() {
            return Err(SonitusError::HttpStatus {
                status: resp.status().as_u16(),
                message: "onedrive ping failed".into(),
            });
        }
        Ok(())
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;

        let mut out = Vec::new();
        let mut to_visit = vec![format!("{GRAPH}/me/drive/root/children?$top=200")];

        while let Some(url) = to_visit.pop() {
            let resp: GraphChildrenResponse = client
                .get(&url)
                .bearer_auth(self.access())
                .send()
                .await
                .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?
                .error_for_status()
                .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?
                .json()
                .await
                .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;

            for item in resp.value {
                if item.folder.is_some() {
                    let folder_url = format!(
                        "{GRAPH}/me/drive/items/{}/children?$top=200",
                        item.id
                    );
                    to_visit.push(folder_url);
                } else if item.file.is_some() {
                    let ext = std::path::Path::new(&item.name)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_ascii_lowercase());
                    if !ext.as_deref().and_then(TrackFormat::from_extension).is_some() { continue; }
                    let path = match item.parent_reference.and_then(|p| p.path) {
                        Some(p) => format!("{p}/{}", item.name),
                        None => format!("/{}", item.name),
                    };
                    out.push(RemoteFile {
                        // Path is the *item ID* — used for streaming.
                        // We embed the visible path as audit context only.
                        path: item.id.clone(),
                        size_bytes: item.size.unwrap_or(0),
                        modified_at: item.last_modified.as_deref().and_then(|s| {
                            chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
                        }),
                        mime_hint: item.file.and_then(|f| f.mime_type),
                    });
                    let _ = path;
                }
            }

            if let Some(next) = resp.next_link { to_visit.push(next); }
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
        let mut req = client
            .get(format!("{GRAPH}/me/drive/items/{path}/content"))
            .bearer_auth(self.access());
        if let Some(r) = range {
            req = req.header("Range", format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;
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
        let resume_from = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
        let mut req = client
            .get(format!("{GRAPH}/me/drive/items/{path}/content"))
            .bearer_auth(self.access());
        if resume_from > 0 {
            req = req.header("Range", format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;
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
            let chunk = chunk.map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;
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
        let resp = client
            .get(format!("{GRAPH}/me/drive/items/{path}/content"))
            .bearer_auth(self.access())
            .header("Range", format!("bytes=0-{}", max_bytes.saturating_sub(1)))
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?;
        Ok(resp.bytes().await
            .map_err(|e| SonitusError::Source { kind: "onedrive", message: e.to_string() })?)
    }
}
