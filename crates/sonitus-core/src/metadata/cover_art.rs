//! Cover art helpers — extract from tags, dedupe by hash, optional fetch
//! from external URL (consent-gated through MusicBrainz/CAA).

use crate::error::Result;

/// One piece of cover art with metadata.
#[derive(Debug, Clone)]
pub struct CoverArt {
    /// Raw image bytes (JPEG/PNG/etc.).
    pub bytes: Vec<u8>,
    /// MIME type if known (`"image/jpeg"`, `"image/png"`).
    pub mime: Option<String>,
    /// BLAKE3 hex hash for dedup.
    pub hash: String,
}

impl CoverArt {
    /// Wrap image bytes, computing the dedup hash.
    pub fn from_bytes(bytes: Vec<u8>, mime: Option<String>) -> Self {
        let hash = blake3::hash(&bytes).to_hex().to_string();
        Self { bytes, mime, hash }
    }

    /// Sniff the MIME type from the magic bytes.
    pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
        if bytes.starts_with(b"\xFF\xD8\xFF") { return Some("image/jpeg"); }
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return Some("image/png"); }
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { return Some("image/gif"); }
        if bytes.starts_with(b"RIFF") && bytes.len() > 11 && &bytes[8..12] == b"WEBP" {
            return Some("image/webp");
        }
        None
    }
}

/// Resize an embedded cover art blob to a thumbnail (best-effort, in-memory).
/// We don't pull `image` crate yet; this is a placeholder for the UI to
/// call when it has a renderer-side resizer (`ImageBitmap`, etc.).
pub fn estimate_dimensions(_bytes: &[u8]) -> Option<(u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_recognizes_jpeg_magic() {
        assert_eq!(CoverArt::sniff_mime(b"\xFF\xD8\xFF\xE0extra"), Some("image/jpeg"));
    }

    #[test]
    fn sniff_recognizes_png_magic() {
        assert_eq!(CoverArt::sniff_mime(b"\x89PNG\r\n\x1a\nIHDR"), Some("image/png"));
    }

    #[test]
    fn sniff_returns_none_for_unknown() {
        assert_eq!(CoverArt::sniff_mime(b"random bytes"), None);
    }

    #[test]
    fn from_bytes_computes_hash() {
        let a = CoverArt::from_bytes(vec![1, 2, 3], None);
        let b = CoverArt::from_bytes(vec![1, 2, 3], None);
        assert_eq!(a.hash, b.hash);
    }
}
