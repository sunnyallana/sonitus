//! Tag parsing for the formats Sonitus indexes.
//!
//! Strategy: try format-specific parsers in order of likely accuracy:
//!
//! 1. [`id3`] for MP3 / WAV.
//! 2. [`metaflac`] for FLAC.
//! 3. [`probe`](super::probe) (Symphonia) for everything else, which
//!    extracts limited tags from MP4 atoms, OGG comments, etc.
//!
//! Each parser populates a [`ParsedTags`] struct. The scanner takes
//! whichever fields are present and uses them; missing fields stay `None`.

use crate::error::{Result, SonitusError};

/// Aggregated parsed metadata from any tag format.
#[derive(Debug, Clone, Default)]
pub struct ParsedTags {
    /// Track title.
    pub title: Option<String>,
    /// Primary artist (track-level).
    pub artist: Option<String>,
    /// Album-artist (used for compilations).
    pub album_artist: Option<String>,
    /// Album title.
    pub album: Option<String>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Track number within disc.
    pub track_number: Option<i32>,
    /// Disc number.
    pub disc_number: Option<i32>,
    /// Total tracks per album metadata.
    pub total_tracks: Option<i32>,
    /// Beats per minute.
    pub bpm: Option<f64>,
    /// Track-level ReplayGain in dB.
    pub replay_gain_track: Option<f64>,
    /// Album-level ReplayGain in dB.
    pub replay_gain_album: Option<f64>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Bitrate in kbps.
    pub bitrate_kbps: Option<i32>,
    /// Sample rate in Hz.
    pub sample_rate_hz: Option<i32>,
    /// Bit depth in bits per sample.
    pub bit_depth: Option<i32>,
    /// Number of channels.
    pub channels: Option<i32>,
    /// Embedded cover art bytes.
    pub cover_art: Option<Vec<u8>>,
}

impl ParsedTags {
    /// Filename-only fallback: split on common patterns ("Artist - Title").
    pub fn guess_from_filename(path: &str) -> Self {
        let stem = path
            .rsplit(['/', '\\']).next().unwrap_or(path)
            .rsplit_once('.').map(|(s, _)| s).unwrap_or(path);
        let mut out = Self::default();
        if let Some((artist, title)) = stem.split_once(" - ") {
            out.artist = Some(artist.trim().to_string());
            out.title = Some(title.trim().to_string());
        } else {
            out.title = Some(stem.to_string());
        }
        out
    }

    fn parse_year(s: &str) -> Option<i32> {
        // Year tags are sometimes "2003-01-15", "2003", or "©2003".
        let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
        cleaned.parse().ok()
    }

    fn parse_track_number(s: &str) -> (Option<i32>, Option<i32>) {
        // "3/12" → (Some(3), Some(12)); "3" → (Some(3), None).
        if let Some((a, b)) = s.split_once('/') {
            (a.trim().parse().ok(), b.trim().parse().ok())
        } else {
            (s.trim().parse().ok(), None)
        }
    }
}

/// Parse tags from `bytes` for the file at `path`. Format is sniffed from
/// the extension. Falls back to filename-based guessing on error.
pub fn parse(path: &str, bytes: &[u8]) -> Result<ParsedTags> {
    let ext = path
        .rsplit('.').next()
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("mp3") | Some("wav") | Some("aiff") | Some("aif") => parse_id3(bytes),
        Some("flac") => parse_flac(bytes),
        _ => parse_with_probe(bytes),
    }
}

/// Parse ID3 tags (MP3, WAV with ID3 chunk).
pub fn parse_id3(bytes: &[u8]) -> Result<ParsedTags> {
    // id3 1.16 moved the convenience getters (`title`, `artist`, etc.) into the
    // `TagLike` trait — bring it into scope so they resolve on `Tag`.
    use id3::TagLike;
    let cursor = std::io::Cursor::new(bytes);
    let tag = id3::Tag::read_from2(cursor)
        .map_err(|e| SonitusError::Audio(format!("id3 parse: {e}")))?;
    let mut out = ParsedTags::default();
    out.title = tag.title().map(str::to_string);
    out.artist = tag.artist().map(str::to_string);
    out.album_artist = tag.album_artist().map(str::to_string);
    out.album = tag.album().map(str::to_string);
    out.genre = tag.genre().map(str::to_string);
    out.year = tag.year();
    out.track_number = tag.track().map(|t| t as i32);
    out.disc_number = tag.disc().map(|d| d as i32);
    out.total_tracks = tag.total_tracks().map(|t| t as i32);

    // ReplayGain frames: TXXX:REPLAYGAIN_TRACK_GAIN / REPLAYGAIN_ALBUM_GAIN.
    for frame in tag.extended_texts() {
        let key = frame.description.to_ascii_uppercase();
        let value = frame.value.trim_end_matches(" dB").trim();
        if key == "REPLAYGAIN_TRACK_GAIN" {
            out.replay_gain_track = value.parse().ok();
        } else if key == "REPLAYGAIN_ALBUM_GAIN" {
            out.replay_gain_album = value.parse().ok();
        }
    }

    // Embedded cover art.
    if let Some(pic) = tag.pictures().next() {
        out.cover_art = Some(pic.data.clone());
    }

    Ok(out)
}

/// Parse FLAC Vorbis comments via `metaflac`.
pub fn parse_flac(bytes: &[u8]) -> Result<ParsedTags> {
    // metaflac 0.2's read_from takes `&mut dyn Read`. Bind the cursor mutably
    // and pass `&mut`.
    let mut cursor = std::io::Cursor::new(bytes);
    let tag = metaflac::Tag::read_from(&mut cursor)
        .map_err(|e| SonitusError::Audio(format!("flac parse: {e}")))?;
    let mut out = ParsedTags::default();

    if let Some(vc) = tag.vorbis_comments() {
        out.title = vc.title().and_then(|t| t.first().cloned());
        out.artist = vc.artist().and_then(|a| a.first().cloned());
        out.album = vc.album().and_then(|a| a.first().cloned());
        out.album_artist = vc.album_artist().and_then(|a| a.first().cloned());
        if let Some(g) = vc.get("GENRE").and_then(|v| v.first()) { out.genre = Some(g.clone()); }
        if let Some(y) = vc.get("DATE").and_then(|v| v.first()) {
            out.year = ParsedTags::parse_year(y);
        }
        if let Some(tn) = vc.get("TRACKNUMBER").and_then(|v| v.first()) {
            let (n, t) = ParsedTags::parse_track_number(tn);
            out.track_number = n;
            if t.is_some() { out.total_tracks = t; }
        }
        if let Some(dn) = vc.get("DISCNUMBER").and_then(|v| v.first()) {
            out.disc_number = dn.parse().ok();
        }
        if let Some(rg) = vc.get("REPLAYGAIN_TRACK_GAIN").and_then(|v| v.first()) {
            out.replay_gain_track = rg.trim_end_matches(" dB").trim().parse().ok();
        }
        if let Some(rg) = vc.get("REPLAYGAIN_ALBUM_GAIN").and_then(|v| v.first()) {
            out.replay_gain_album = rg.trim_end_matches(" dB").trim().parse().ok();
        }
        if let Some(b) = vc.get("BPM").and_then(|v| v.first()) {
            out.bpm = b.parse().ok();
        }
    }

    if let Some(si) = tag.get_streaminfo() {
        out.sample_rate_hz = Some(si.sample_rate as i32);
        out.bit_depth = Some(si.bits_per_sample as i32);
        out.channels = Some(si.num_channels as i32);
        if si.sample_rate > 0 {
            out.duration_ms = Some(((si.total_samples as f64 / si.sample_rate as f64) * 1000.0) as i64);
        }
    }

    if let Some(pic) = tag.pictures().next() {
        out.cover_art = Some(pic.data.clone());
    }

    Ok(out)
}

/// Fallback to Symphonia probe-based extraction for non-MP3/FLAC formats.
pub fn parse_with_probe(bytes: &[u8]) -> Result<ParsedTags> {
    super::probe::extract_tags(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_from_filename_splits_artist_dash_title() {
        let p = ParsedTags::guess_from_filename("/Music/Pink Floyd - Time.mp3");
        assert_eq!(p.artist.as_deref(), Some("Pink Floyd"));
        assert_eq!(p.title.as_deref(), Some("Time"));
    }

    #[test]
    fn guess_from_filename_falls_back_to_stem_only() {
        let p = ParsedTags::guess_from_filename("song.mp3");
        assert_eq!(p.title.as_deref(), Some("song"));
        assert!(p.artist.is_none());
    }

    #[test]
    fn parse_year_extracts_first_four_digits() {
        assert_eq!(ParsedTags::parse_year("2003"), Some(2003));
        assert_eq!(ParsedTags::parse_year("2003-01-15"), Some(2003));
        assert_eq!(ParsedTags::parse_year("©2003"), Some(2003));
        assert_eq!(ParsedTags::parse_year("not a year"), None);
    }

    #[test]
    fn parse_track_number_handles_slash_form() {
        assert_eq!(ParsedTags::parse_track_number("3/12"), (Some(3), Some(12)));
        assert_eq!(ParsedTags::parse_track_number("3"), (Some(3), None));
        assert_eq!(ParsedTags::parse_track_number(""), (None, None));
    }
}
