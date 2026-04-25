//! Google Drive source — Drive API v3 + OAuth2 PKCE.
//!
//! ## Auth flow
//!
//! 1. UI calls [`GoogleDriveSource::begin_oauth`] which returns an
//!    authorization URL.
//! 2. User opens the URL in their browser, grants access, gets redirected
//!    to `http://localhost:8888/callback?code=...`.
//! 3. Sonitus catches the callback (a tiny localhost listener), exchanges
//!    the code for an access + refresh token, and persists them in the
//!    encrypted credential vault.
//!
//! ## Listing
//!
//! Drive API v3 `files.list` is paginated (`nextPageToken`). We loop until
//! the token is empty. Files are filtered server-side via `q="mimeType
//! contains 'audio/'"` which matches MP3, FLAC, AAC, OGG, WAV, M4A.

use crate::crypto::Secret;
use crate::error::{Result, SonitusError};
use crate::library::models::SourceKind;
use crate::privacy::{AuditLogger, TriggerSource, http_client};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;
use parking_lot::RwLock;

const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Google Drive source.
pub struct GoogleDriveSource {
    id: String,
    name: String,
    /// Restrict scan to a specific folder ID, or `None` for the entire drive.
    root_folder_id: Option<String>,
    audit: Arc<AuditLogger>,
    /// In-memory cache of the access + refresh tokens. Decrypted from
    /// the DB on construction. Wrapped in RwLock so refresh can update.
    tokens: RwLock<TokenPair>,
    /// OAuth client ID used for token refresh.
    client_id: String,
    /// OAuth client secret (yes, Drive's PKCE flow still wants this).
    client_secret: Secret<String>,
}

#[derive(Debug, Clone)]
struct TokenPair {
    access: Secret<String>,
    refresh: Option<Secret<String>>,
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct DriveListResponse {
    files: Vec<DriveFileItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct DriveFileItem {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    size: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

impl GoogleDriveSource {
    /// Construct a new source given existing credentials. The orchestrator
    /// should decrypt the credentials and pass them in.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        root_folder_id: Option<String>,
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
            root_folder_id,
            audit,
            tokens: RwLock::new(TokenPair {
                access: Secret::new(access_token),
                refresh: refresh_token.map(Secret::new),
                expires_at,
            }),
            client_id,
            client_secret: Secret::new(client_secret),
        }
    }

    /// Refresh the access token using the refresh token. Updates the
    /// in-memory tokens but not the DB — the caller is responsible for
    /// persisting after this returns.
    pub async fn refresh_token(&self) -> Result<()> {
        let refresh = {
            let r = self.tokens.read();
            r.refresh.as_ref().map(|t| t.expose().clone())
        };
        let Some(refresh) = refresh else {
            return Err(SonitusError::NotAuthenticated(self.id.clone()));
        };
        let client = http_client(self.audit.clone(), TriggerSource::OauthRefresh)?;
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.expose().as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ];
        let resp = client
            .post(TOKEN_URL)
            .form(&params)
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
        if let Some(new_refresh) = tok.refresh_token {
            w.refresh = Some(Secret::new(new_refresh));
        }
        if let Some(exp_in) = tok.expires_in {
            w.expires_at = Some(chrono::Utc::now().timestamp() + exp_in);
        }
        Ok(())
    }

    fn current_access(&self) -> String {
        self.tokens.read().access.expose().clone()
    }

    async fn ensure_fresh(&self) -> Result<()> {
        let needs_refresh = {
            let r = self.tokens.read();
            r.expires_at
                .map(|exp| chrono::Utc::now().timestamp() + 60 > exp)
                .unwrap_or(false)
        };
        if needs_refresh {
            self.refresh_token().await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SourceProvider for GoogleDriveSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::GoogleDrive }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::UserAction)?;
        let resp = client
            .get(format!("{DRIVE_API}/about?fields=user"))
            .bearer_auth(self.current_access())
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
        if !resp.status().is_success() {
            return Err(SonitusError::HttpStatus {
                status: resp.status().as_u16(),
                message: "drive ping failed".into(),
            });
        }
        Ok(())
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;
        let mut q = String::from("mimeType contains 'audio/' and trashed = false");
        if let Some(root) = &self.root_folder_id {
            q = format!("'{root}' in parents and {q}");
        }

        let mut out = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut req = client
                .get(format!("{DRIVE_API}/files"))
                .bearer_auth(self.current_access())
                .query(&[
                    ("q", q.as_str()),
                    ("fields", "nextPageToken,files(id,name,mimeType,size,modifiedTime)"),
                    ("pageSize", "1000"),
                ]);
            if let Some(t) = &page_token {
                req = req.query(&[("pageToken", t.as_str())]);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?
                .error_for_status()
                .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
            let body: DriveListResponse = resp
                .json()
                .await
                .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;

            for f in body.files {
                out.push(RemoteFile {
                    path: f.id.clone(), // Drive paths ARE the file IDs
                    size_bytes: f.size.and_then(|s| s.parse().ok()).unwrap_or(0),
                    modified_at: f.modified_time.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.timestamp())
                    }),
                    mime_hint: Some(f.mime_type),
                });
                let _ = f.name; // currently we use IDs for paths; name kept in tags
            }

            page_token = body.next_page_token;
            if page_token.is_none() { break; }
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
            .get(format!("{DRIVE_API}/files/{path}?alt=media"))
            .bearer_auth(self.current_access());
        if let Some(r) = range {
            req = req.header("Range", format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
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
            .get(format!("{DRIVE_API}/files/{path}?alt=media"))
            .bearer_auth(self.current_access());
        if resume_from > 0 {
            req = req.header("Range", format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
        let total = resp.content_length().map(|c| c + resume_from);
        if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await?;
        let mut stream = resp.bytes_stream();
        let mut done = resume_from;
        let started = std::time::Instant::now();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
            tokio::io::AsyncWriteExt::write_all(&mut output, &chunk).await?;
            done += chunk.len() as u64;
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed = ((done - resume_from) as f64 / elapsed) as u64;
            let _ = progress
                .send(DownloadProgress {
                    bytes_done: done,
                    bytes_total: total,
                    speed_bps: Some(speed),
                })
                .await;
        }
        Ok(())
    }

    async fn read_bytes(&self, path: &str, max_bytes: usize) -> Result<Bytes> {
        self.ensure_fresh().await?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;
        let resp = client
            .get(format!("{DRIVE_API}/files/{path}?alt=media"))
            .bearer_auth(self.current_access())
            .header("Range", format!("bytes=0-{}", max_bytes.saturating_sub(1)))
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?;
        Ok(resp
            .bytes()
            .await
            .map_err(|e| SonitusError::Source { kind: "google_drive", message: e.to_string() })?)
    }
}

/// Begin OAuth2 PKCE flow for Google Drive. Returns the URL the user
/// should open in their browser, plus the PKCE verifier the caller must
/// keep until the redirect arrives.
pub fn begin_oauth(client_id: &str, redirect_uri: &str) -> Result<(String, String, String)> {
    use oauth2::{
        AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl,
        Scope, basic::BasicClient,
    };
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
            .map_err(|e| SonitusError::OAuth(e.to_string()))?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())
            .map_err(|e| SonitusError::OAuth(e.to_string()))?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("https://www.googleapis.com/auth/drive.readonly".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();
    Ok((auth_url.to_string(), pkce_verifier.secret().to_string(), state.secret().to_string()))
}
