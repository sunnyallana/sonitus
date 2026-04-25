//! Audio metadata: tag parsing, codec probing, optional online lookups.
//!
//! - [`tags`] — Synchronous tag parsers for ID3, FLAC/Vorbis, MP4 atoms.
//! - [`probe`] — Symphonia codec/format detection.
//! - [`cover_art`] — Extract embedded art and resolve external URLs.
//! - [`musicbrainz`] — Optional MusicBrainz API v2 lookups (consent-gated).
//! - [`acoustid`] — Optional AcoustID fingerprint matching (consent-gated).

pub mod acoustid;
pub mod cover_art;
pub mod musicbrainz;
pub mod probe;
pub mod tags;
