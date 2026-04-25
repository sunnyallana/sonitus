//! Symphonia probe — codec/format detection and tag fallback.
//!
//! Used by the scanner whenever we don't have a format-specific parser
//! (e.g. MP4/M4A, OGG, Opus). Symphonia parses container metadata and
//! exposes vendor tags via the standard `Tag` interface.

use crate::error::{Result, SonitusError};
use crate::metadata::tags::ParsedTags;

/// Run a Symphonia probe over `bytes` and lift any tags into `ParsedTags`.
pub fn extract_tags(bytes: &[u8]) -> Result<ParsedTags> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| SonitusError::Audio(format!("symphonia probe: {e}")))?;

    let mut out = ParsedTags::default();
    let mut format = probed.format;

    // Container metadata.
    let mut meta = format.metadata();
    if let Some(rev) = meta.skip_to_latest() {
        for tag in rev.tags() {
            apply_tag(&mut out, tag);
        }
    }

    // Codec params for the default audio track.
    if let Some(track) = format.default_track() {
        let cp = &track.codec_params;
        if let Some(rate) = cp.sample_rate {
            out.sample_rate_hz = Some(rate as i32);
        }
        if let Some(bits) = cp.bits_per_sample {
            out.bit_depth = Some(bits as i32);
        }
        if let Some(ch) = cp.channels.map(|c| c.count()) {
            out.channels = Some(ch as i32);
        }
        if let (Some(n), Some(rate)) = (cp.n_frames, cp.sample_rate) {
            if rate > 0 {
                out.duration_ms = Some(((n as f64 / rate as f64) * 1000.0) as i64);
            }
        }
    }
    Ok(out)
}

fn apply_tag(out: &mut ParsedTags, tag: &symphonia::core::meta::Tag) {
    use symphonia::core::meta::StandardTagKey as K;
    let value_str = tag.value.to_string();
    if let Some(k) = tag.std_key {
        match k {
            K::TrackTitle => out.title = Some(value_str.clone()),
            K::Artist => out.artist = Some(value_str.clone()),
            K::AlbumArtist => out.album_artist = Some(value_str.clone()),
            K::Album => out.album = Some(value_str.clone()),
            K::Genre => out.genre = Some(value_str.clone()),
            K::Date => out.year = parse_year(&value_str),
            K::TrackNumber => {
                let (n, t) = parse_track_number(&value_str);
                out.track_number = n;
                if t.is_some() {
                    out.total_tracks = t;
                }
            }
            K::DiscNumber => out.disc_number = value_str.parse().ok(),
            K::Bpm => out.bpm = value_str.parse().ok(),
            K::ReplayGainTrackGain => {
                out.replay_gain_track = value_str.trim_end_matches(" dB").trim().parse().ok();
            }
            K::ReplayGainAlbumGain => {
                out.replay_gain_album = value_str.trim_end_matches(" dB").trim().parse().ok();
            }
            _ => {}
        }
    }
}

fn parse_year(s: &str) -> Option<i32> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    cleaned.parse().ok()
}

fn parse_track_number(s: &str) -> (Option<i32>, Option<i32>) {
    if let Some((a, b)) = s.split_once('/') {
        (a.trim().parse().ok(), b.trim().parse().ok())
    } else {
        (s.trim().parse().ok(), None)
    }
}
