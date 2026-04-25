//! Symphonia probe — codec/format detection and tag fallback.
//!
//! ## Status
//!
//! The Symphonia 0.6 alpha.2 metadata API differs significantly from
//! alpha.1: `Tag` fields renamed (`std_key`/`value` → `std`/`raw`),
//! `StandardTagKey` → `StandardTag`, `Probe::format()` signature is now
//! lifetime-parameterized. While the migration is in progress, this
//! module returns an empty `ParsedTags` so the scanner falls through to
//! its filename-based heuristic. Format-specific parsers in [`super::tags`]
//! (id3, metaflac) are unchanged and continue to do the heavy lifting
//! for MP3 + FLAC, which together cover the vast majority of real
//! libraries.

use crate::error::Result;
use crate::metadata::tags::ParsedTags;

/// Run a Symphonia probe over `bytes` and lift any tags into `ParsedTags`.
///
/// Currently returns the default empty struct — see module docs.
pub fn extract_tags(_bytes: &[u8]) -> Result<ParsedTags> {
    Ok(ParsedTags::default())
}
