//! Generic HTTP file server source.
//!
//! Works against any HTTP server that:
//!
//! - Returns an HTML directory index for directory URLs (most do —
//!   nginx, Apache, lighttpd, caddy auto-index).
//! - Supports `Range:` requests on file URLs.
//!
//! Every request goes through [`http_client`](crate::privacy::http_client)
//! so it shows up in the audit log.

use crate::error::{Result, SonitusError};
use crate::library::models::{SourceKind, TrackFormat};
use crate::privacy::{AuditLogger, TriggerSource, http_client};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;
use url::Url;

/// HTTP directory-index source.
pub struct HttpSource {
    id: String,
    name: String,
    base_url: Url,
    audit: Arc<AuditLogger>,
}

impl HttpSource {
    /// Construct a new HTTP source rooted at `base_url`.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: Url,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self { id: id.into(), name: name.into(), base_url, audit }
    }

    fn url_for(&self, path: &str) -> Result<Url> {
        let cleaned = path.trim_start_matches('/');
        self.base_url
            .join(cleaned)
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })
    }
}

#[async_trait]
impl SourceProvider for HttpSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::Http }
    fn name(&self) -> &str { &self.name }

    async fn ping(&self) -> Result<()> {
        let client = http_client(self.audit.clone(), TriggerSource::UserAction)?;
        let resp = client
            .head(self.base_url.as_str())
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
        if !resp.status().is_success() && resp.status().as_u16() != 405 {
            return Err(SonitusError::HttpStatus {
                status: resp.status().as_u16(),
                message: format!("HEAD {} failed", self.base_url),
            });
        }
        Ok(())
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;
        let mut to_visit = vec![self.base_url.clone()];
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out = Vec::new();

        while let Some(dir_url) = to_visit.pop() {
            if !visited.insert(dir_url.to_string()) { continue; }

            let resp = client
                .get(dir_url.as_str())
                .send()
                .await
                .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
            if !resp.status().is_success() { continue; }
            let body = resp
                .text()
                .await
                .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;

            for href in extract_hrefs(&body) {
                if href.starts_with('?') || href == "../" { continue; }
                let Ok(abs) = dir_url.join(&href) else { continue; };
                if abs.as_str().starts_with(self.base_url.as_str()) {
                    if href.ends_with('/') {
                        to_visit.push(abs);
                    } else {
                        let path = abs.path();
                        let ext = path.rsplit('.').next();
                        if ext.and_then(TrackFormat::from_extension).is_some() {
                            // HEAD for size.
                            let size_bytes = client
                                .head(abs.as_str())
                                .send()
                                .await
                                .ok()
                                .and_then(|r| r.headers().get("content-length").cloned())
                                .and_then(|v| v.to_str().ok().and_then(|s| s.parse().ok()))
                                .unwrap_or(0);
                            // Make the path relative to base.
                            let rel = abs
                                .as_str()
                                .strip_prefix(self.base_url.as_str())
                                .unwrap_or(abs.path())
                                .to_string();
                            out.push(RemoteFile {
                                path: format!("/{}", rel.trim_start_matches('/')),
                                size_bytes,
                                modified_at: None,
                                mime_hint: None,
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let url = self.url_for(path)?;
        let client = http_client(self.audit.clone(), TriggerSource::Playback)?;
        let mut req = client.get(url.as_str());
        if let Some(r) = range {
            req = req.header("Range", format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
        let stream = resp.bytes_stream();
        let reader = tokio_util::io::StreamReader::new(stream.map(|r| {
            r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }));
        Ok(Box::new(reader))
    }

    async fn download(
        &self,
        path: &str,
        dest: &Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()> {
        let url = self.url_for(path)?;
        let client = http_client(self.audit.clone(), TriggerSource::Download)?;

        // Resume support: if dest exists, send a Range starting at its size.
        let resume_from = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
        let mut req = client.get(url.as_str());
        if resume_from > 0 {
            req = req.header("Range", format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;

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
                .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
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
        let url = self.url_for(path)?;
        let client = http_client(self.audit.clone(), TriggerSource::BackgroundScan)?;
        let resp = client
            .get(url.as_str())
            .header("Range", format!("bytes=0-{}", max_bytes.saturating_sub(1)))
            .send()
            .await
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?
            .error_for_status()
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
        Ok(resp
            .bytes()
            .await
            .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?)
    }
}

/// Crude HTML href extractor — handles plain Apache/nginx/caddy auto-index pages.
/// Not a full HTML parser; we just look for `href="..."` substrings.
fn extract_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let needle = b"href=\"";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            i += needle.len();
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' { i += 1; }
            if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                out.push(s.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_hrefs_picks_anchor_attributes() {
        let html = r#"<html><a href="song.mp3">song</a><a href="sub/">sub</a></html>"#;
        let hrefs = extract_hrefs(html);
        assert!(hrefs.contains(&"song.mp3".to_string()));
        assert!(hrefs.contains(&"sub/".to_string()));
    }

    #[test]
    fn extract_hrefs_handles_no_anchors() {
        assert!(extract_hrefs("<html></html>").is_empty());
    }
}
