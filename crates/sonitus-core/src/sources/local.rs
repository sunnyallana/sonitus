//! Local filesystem source.
//!
//! The simplest backend: paths are normal OS paths, list is `walkdir`,
//! streaming is `tokio::fs::File`. No auth, no network, no rate limits.

use crate::error::{Result, SonitusError};
use crate::library::models::{SourceKind, TrackFormat};
use crate::sources::{DownloadProgress, RemoteFile, SourceProvider};
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use walkdir::WalkDir;

/// Local filesystem source rooted at a directory.
pub struct LocalSource {
    id: String,
    name: String,
    root: PathBuf,
}

impl LocalSource {
    /// Construct a new local source.
    pub fn new(id: impl Into<String>, name: impl Into<String>, root: PathBuf) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            root,
        }
    }

    /// Resolve a remote path against `self.root`.
    fn resolve(&self, path: &str) -> PathBuf {
        // Trim a leading slash so `Path::join` doesn't wipe the root.
        let stripped = path.trim_start_matches(['/', '\\']);
        self.root.join(stripped)
    }
}

#[async_trait]
impl SourceProvider for LocalSource {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> SourceKind { SourceKind::Local }
    fn name(&self) -> &str { &self.name }

    fn local_path(&self, path: &str) -> Option<std::path::PathBuf> {
        Some(self.resolve(path))
    }

    async fn ping(&self) -> Result<()> {
        if !self.root.exists() {
            return Err(SonitusError::PathNotFound(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(SonitusError::Source {
                kind: "local",
                message: format!("{} is not a directory", self.root.display()),
            });
        }
        Ok(())
    }

    async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        // Same walk as `discover`, but drains into a Vec at the end.
        let root = self.root.clone();
        let files = tokio::task::spawn_blocking(move || walk_audio_files(&root))
            .await
            .map_err(|e| SonitusError::Source { kind: "local", message: e.to_string() })?;
        Ok(files)
    }

    /// Streaming discovery: emits each audio file as walkdir surfaces it,
    /// so the scanner can begin processing immediately rather than waiting
    /// for the full tree walk to finish. Skips the second `fs::metadata`
    /// call by reusing the dirent metadata walkdir already loaded.
    async fn discover(&self, tx: tokio::sync::mpsc::Sender<RemoteFile>) -> Result<()> {
        let root = self.root.clone();
        tracing::info!(root = %root.display(), "local: starting discover walk");
        let join = tokio::task::spawn_blocking(move || -> Result<u64> {
            let mut emitted: u64 = 0;
            let mut visited: u64 = 0;
            for entry in WalkDir::new(&root).follow_links(false).into_iter() {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, "local: walkdir entry error; skipping");
                        continue;
                    }
                };
                visited += 1;
                if !entry.file_type().is_file() {
                    continue;
                }
                let p = entry.path();
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase());
                let Some(ref ext) = ext else {
                    continue;
                };
                if TrackFormat::from_extension(ext).is_none() {
                    continue;
                }
                // Reuse walkdir's dirent metadata; saves one stat per file.
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(path = %p.display(), error = %e, "local: metadata error; skipping");
                        continue;
                    }
                };
                let path_rel = p
                    .strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string();
                let file = RemoteFile {
                    path: format!("/{}", path_rel.replace('\\', "/")),
                    size_bytes: meta.len(),
                    modified_at: meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64),
                    mime_hint: None,
                };
                // blocking_send blocks this dedicated spawn_blocking thread
                // when the channel fills, giving back-pressure without a
                // tokio runtime call from inside the closure.
                if tx.blocking_send(file).is_err() {
                    tracing::warn!("local: receiver dropped; aborting walk");
                    break;
                }
                emitted += 1;
            }
            tracing::info!(visited, emitted, "local: walk done");
            Ok(emitted)
        });
        join.await
            .map_err(|e| SonitusError::Source { kind: "local", message: e.to_string() })??;
        Ok(())
    }

    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let full = self.resolve(path);
        let mut file = tokio::fs::File::open(&full).await?;
        if let Some(r) = range {
            file.seek(std::io::SeekFrom::Start(r.start)).await?;
            // Bound the reader at r.end via take().
            let limited = file.take(r.end.saturating_sub(r.start));
            return Ok(Box::new(limited));
        }
        Ok(Box::new(file))
    }

    async fn download(
        &self,
        path: &str,
        dest: &Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()> {
        let src = self.resolve(path);
        let mut input = tokio::fs::File::open(&src).await?;
        let total = input.metadata().await?.len();
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut output = tokio::fs::File::create(dest).await?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        let started = std::time::Instant::now();
        loop {
            let n = input.read(&mut buf).await?;
            if n == 0 { break; }
            tokio::io::AsyncWriteExt::write_all(&mut output, &buf[..n]).await?;
            done += n as u64;
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed_bps = (done as f64 / elapsed) as u64;
            let _ = progress
                .send(DownloadProgress {
                    bytes_done: done,
                    bytes_total: Some(total),
                    speed_bps: Some(speed_bps),
                })
                .await;
        }
        Ok(())
    }

    async fn read_bytes(&self, path: &str, max_bytes: usize) -> Result<Bytes> {
        let full = self.resolve(path);
        let mut file = tokio::fs::File::open(&full).await?;
        let len = file.metadata().await?.len() as usize;
        let to_read = max_bytes.min(len);
        let mut buf = vec![0u8; to_read];
        file.read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }
}

/// Synchronous walkdir helper shared by `list_files` and `discover`.
/// Pulls audio file metadata from dirents in a single pass.
fn walk_audio_files(root: &std::path::Path) -> Vec<RemoteFile> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        let Some(ref ext) = ext else { continue; };
        if TrackFormat::from_extension(ext).is_none() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let path_rel = p
            .strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .to_string();
        out.push(RemoteFile {
            path: format!("/{}", path_rel.replace('\\', "/")),
            size_bytes: meta.len(),
            modified_at: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            mime_hint: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_dummy_mp3(path: &Path) {
        std::fs::write(path, b"ID3\x04\x00\x00\x00\x00\x00\x00stub mp3 bytes").unwrap();
    }

    #[tokio::test]
    async fn ping_succeeds_on_existing_directory() {
        let dir = TempDir::new().unwrap();
        let src = LocalSource::new("s1", "Test", dir.path().to_path_buf());
        src.ping().await.unwrap();
    }

    #[tokio::test]
    async fn ping_fails_on_missing_directory() {
        let src = LocalSource::new("s1", "Test", PathBuf::from("/this/does/not/exist"));
        let r = src.ping().await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn list_files_finds_audio_files_only() {
        let dir = TempDir::new().unwrap();
        make_dummy_mp3(&dir.path().join("song.mp3"));
        std::fs::write(dir.path().join("notes.txt"), b"text file").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        make_dummy_mp3(&dir.path().join("sub").join("nested.flac"));

        let src = LocalSource::new("s1", "Test", dir.path().to_path_buf());
        let files = src.list_files().await.unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("song.mp3")));
        assert!(paths.iter().any(|p| p.ends_with("nested.flac")));
        assert!(!paths.iter().any(|p| p.ends_with("notes.txt")));
    }

    #[tokio::test]
    async fn read_bytes_returns_first_n_bytes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("track.mp3");
        std::fs::write(&path, b"ABCDEFGHIJ").unwrap();
        let src = LocalSource::new("s1", "Test", dir.path().to_path_buf());
        let b = src.read_bytes("/track.mp3", 5).await.unwrap();
        assert_eq!(&b[..], b"ABCDE");
    }
}
