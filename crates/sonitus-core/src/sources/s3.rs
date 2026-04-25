//! AWS S3 (or any S3-compatible service: MinIO, Backblaze B2, Wasabi, R2).
//!
//! Uses `aws-sdk-s3` directly. Auth via static `(access_key, secret_key)`
//! supplied by the user — we never use AWS env vars or instance metadata,
//! both of which could leak credentials beyond what the user intends.
//!
//! ## Audit logging
//!
//! `aws-sdk-s3` does not use `reqwest_middleware`, so requests it makes do
//! not flow through `AuditMiddleware`. We add an explicit audit-log call
//! around every S3 op to maintain guarantee ⑤. (A future refactor could
//! plug a custom AWS HTTP connector.)

#![cfg(feature = "s3")]

use crate::error::{Result, SonitusError};
use crate::library::models::SourceKind;
use crate::privacy::{AuditEntry, AuditLogger, TriggerSource, audit::redact_query};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Credentials;
use bytes::Bytes;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;

/// AWS S3 (or compatible) source.
pub struct S3Source {
    id: String,
    name: String,
    bucket: String,
    /// Optional prefix to scope to a "folder" inside the bucket.
    prefix: String,
    client: Client,
    audit: Arc<AuditLogger>,
    /// Region as a string, used for audit log destination.
    endpoint_host: String,
}

impl S3Source {
    /// Construct an S3 source with explicit credentials.
    /// `endpoint_url` lets you point at non-AWS providers (MinIO etc.).
    pub async fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        bucket: impl Into<String>,
        prefix: impl Into<String>,
        access_key: String,
        secret_key: String,
        region: String,
        endpoint_url: Option<String>,
        audit: Arc<AuditLogger>,
    ) -> Result<Self> {
        let creds = Credentials::new(access_key, secret_key, None, None, "sonitus-static");
        let mut conf = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(creds)
            .region(Region::new(region.clone()));
        if let Some(ep) = &endpoint_url {
            conf = conf.endpoint_url(ep.clone());
        }
        let shared = conf.load().await;
        let client = Client::new(&shared);

        let host = endpoint_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| format!("s3.{region}.amazonaws.com"));

        Ok(Self {
            id: id.into(),
            name: name.into(),
            bucket: bucket.into(),
            prefix: prefix.into(),
            client,
            audit,
            endpoint_host: host,
        })
    }

    fn audit_record(&self, method: &str, path: &str, by: TriggerSource, ms: u64, status: Option<u16>, error: Option<String>) {
        let _ = self.audit.append(&AuditEntry {
            ts: Utc::now(),
            dest: self.endpoint_host.clone(),
            method: method.into(),
            path: redact_query(path),
            by,
            sent: 0,
            recv: 0,
            status,
            ms,
            error,
        });
    }
}

#[async_trait]
impl SourceProvider for S3Source {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::S3 }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        let started = std::time::Instant::now();
        let res = self.client.head_bucket().bucket(&self.bucket).send().await;
        let ms = started.elapsed().as_millis() as u64;
        match res {
            Ok(_) => {
                self.audit_record("HEAD", &format!("/{}", self.bucket), TriggerSource::UserAction, ms, Some(200), None);
                Ok(())
            }
            Err(e) => {
                self.audit_record("HEAD", &format!("/{}", self.bucket), TriggerSource::UserAction, ms, None, Some(e.to_string()));
                Err(SonitusError::Source { kind: "s3", message: e.to_string() })
            }
        }
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        let mut out = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let started = std::time::Instant::now();
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix);
            if let Some(c) = &continuation {
                req = req.continuation_token(c);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| SonitusError::Source { kind: "s3", message: e.to_string() })?;
            let ms = started.elapsed().as_millis() as u64;
            self.audit_record("GET", &format!("/{}?list-type=2", self.bucket), TriggerSource::BackgroundScan, ms, Some(200), None);

            for obj in resp.contents() {
                let Some(key) = obj.key() else { continue; };
                // Filter by extension on our side.
                let ext = std::path::Path::new(key)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase());
                let Some(ref ext) = ext else { continue; };
                if crate::library::models::TrackFormat::from_extension(ext).is_none() { continue; }
                out.push(RemoteFile {
                    path: format!("/{key}"),
                    size_bytes: obj.size().unwrap_or(0).max(0) as u64,
                    modified_at: obj.last_modified().map(|t| t.secs()),
                    mime_hint: None,
                });
            }

            continuation = resp.next_continuation_token().map(str::to_string);
            if !resp.is_truncated().unwrap_or(false) || continuation.is_none() {
                break;
            }
        }
        Ok(out)
    }

    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let key = path.trim_start_matches('/');
        let started = std::time::Instant::now();
        let mut req = self.client.get_object().bucket(&self.bucket).key(key);
        if let Some(r) = range {
            req = req.range(format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "s3", message: e.to_string() })?;
        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("GET", &format!("/{}/{}", self.bucket, key), TriggerSource::Playback, ms, Some(200), None);
        Ok(Box::new(resp.body.into_async_read()))
    }

    async fn download(
        &self,
        path: &str,
        dest: &Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()> {
        let key = path.trim_start_matches('/');
        let resume_from = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);

        let started = std::time::Instant::now();
        let mut req = self.client.get_object().bucket(&self.bucket).key(key);
        if resume_from > 0 {
            req = req.range(format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "s3", message: e.to_string() })?;
        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("GET", &format!("/{}/{}", self.bucket, key), TriggerSource::Download, ms, Some(200), None);
        let total = resp.content_length().map(|c| c.max(0) as u64 + resume_from);

        if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await?;
        let mut body = resp.body.into_async_read();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = resume_from;
        let dl_start = std::time::Instant::now();
        loop {
            let n = tokio::io::AsyncReadExt::read(&mut body, &mut buf).await?;
            if n == 0 { break; }
            tokio::io::AsyncWriteExt::write_all(&mut output, &buf[..n]).await?;
            done += n as u64;
            let elapsed = dl_start.elapsed().as_secs_f64().max(0.001);
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
        let key = path.trim_start_matches('/');
        let started = std::time::Instant::now();
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .range(format!("bytes=0-{}", max_bytes.saturating_sub(1)))
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "s3", message: e.to_string() })?;
        let ms = started.elapsed().as_millis() as u64;
        self.audit_record("GET", &format!("/{}/{}", self.bucket, key), TriggerSource::BackgroundScan, ms, Some(200), None);
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| SonitusError::Source { kind: "s3", message: e.to_string() })?;
        Ok(bytes.into_bytes())
    }
}
