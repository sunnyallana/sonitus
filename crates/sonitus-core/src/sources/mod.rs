//! Pluggable storage sources.
//!
//! Sonitus indexes music wherever it already lives. Each source kind
//! implements [`SourceProvider`], which abstracts file listing,
//! byte-range streaming, and full downloads. The library scanner
//! traverses sources via the trait without caring about the underlying
//! protocol.
//!
//! ## Implementations
//!
//! | Kind         | Module       | Auth         |
//! |--------------|--------------|--------------|
//! | Local        | [`local`]    | None         |
//! | Google Drive | [`google_drive`] | OAuth2 PKCE |
//! | S3           | [`s3`]       | Access key   |
//! | SMB          | [`smb`]      | User+pass    |
//! | HTTP         | [`http`]     | Optional Basic |
//! | Dropbox      | [`dropbox`]  | OAuth2       |
//! | OneDrive     | [`onedrive`] | OAuth2       |

pub mod local;

#[cfg(feature = "smb")]
pub mod smb;

#[cfg(feature = "s3")]
pub mod s3;

pub mod dropbox;
pub mod google_drive;
pub mod http;
pub mod onedrive;

use crate::error::Result;
use crate::library::models::SourceKind;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncRead;

/// Metadata about a single file at the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    /// Path within the source. Format is source-specific (POSIX-y for
    /// local/SMB/HTTP, opaque file IDs for cloud sources, with a path
    /// hint provided alongside).
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Last modification time, Unix epoch seconds. Optional.
    pub modified_at: Option<i64>,
    /// MIME type hint, if the source provides one.
    pub mime_hint: Option<String>,
}

/// Progress report for a download in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Bytes received so far.
    pub bytes_done: u64,
    /// Total bytes if known.
    pub bytes_total: Option<u64>,
    /// Instantaneous speed, in bytes/sec, exponentially smoothed.
    pub speed_bps: Option<u64>,
}

/// Trait every source backend implements.
#[async_trait]
pub trait SourceProvider: Send + Sync + 'static {
    /// Stable identifier (matches the row in `sources` table).
    fn id(&self) -> &str;

    /// What kind of source this is.
    fn kind(&self) -> SourceKind;

    /// User-visible name.
    fn name(&self) -> &str;

    /// Test connectivity. Called before first scan and from the
    /// settings UI's "Test connection" button.
    async fn ping(&self) -> Result<()>;

    /// Recursively list every audio file under the source's root.
    async fn list_files(&self) -> Result<Vec<RemoteFile>>;

    /// Open a streaming reader for `path`, optionally restricted to a
    /// byte range. Range support is required for efficient seeking.
    async fn stream(
        &self,
        path: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>>;

    /// Download `path` to `dest`, reporting progress on the channel.
    /// Implementations should support resume by inspecting an existing
    /// `dest` file and using a Range request.
    async fn download(
        &self,
        path: &str,
        dest: &std::path::Path,
        progress: tokio::sync::mpsc::Sender<DownloadProgress>,
    ) -> Result<()>;

    /// Read the first `max_bytes` of `path`. Used to extract tags
    /// without downloading the entire file.
    async fn read_bytes(&self, path: &str, max_bytes: usize) -> Result<Bytes>;
}
