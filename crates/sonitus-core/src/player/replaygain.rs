//! ReplayGain (EBU R128) loudness normalization.
//!
//! ReplayGain is a tagging convention: each track (and optionally each
//! album) has a `replay_gain_track` / `replay_gain_album` value in
//! decibels recorded in its tags. To normalize playback we multiply each
//! sample by `10^(gain_db / 20)`.
//!
//! This module is the audio-thread side: pure linear gain. The metadata
//! parser pulls the `REPLAYGAIN_TRACK_GAIN` / `REPLAYGAIN_ALBUM_GAIN`
//! tag values during scanning.

use crate::config::ReplayGainMode;

/// Per-track gain values pulled from tags.
#[derive(Debug, Clone, Copy, Default)]
pub struct GainValues {
    /// Track-level gain in dB (`REPLAYGAIN_TRACK_GAIN`).
    pub track_db: Option<f64>,
    /// Album-level gain in dB (`REPLAYGAIN_ALBUM_GAIN`).
    pub album_db: Option<f64>,
}

/// Compute the linear amplitude factor to apply, given mode + values.
/// Returns 1.0 if normalization is off or tags are missing.
pub fn linear_gain(mode: ReplayGainMode, values: GainValues) -> f32 {
    let db = match mode {
        ReplayGainMode::Off => return 1.0,
        ReplayGainMode::Track => values.track_db.or(values.album_db),
        ReplayGainMode::Album => values.album_db.or(values.track_db),
    };
    let Some(db) = db else { return 1.0; };
    10f64.powf(db / 20.0) as f32
}

/// Apply gain in place to a slice of f32 samples (interleaved, any layout).
pub fn apply_gain(samples: &mut [f32], gain: f32) {
    if (gain - 1.0).abs() < f32::EPSILON {
        return; // unity — no work to do
    }
    for s in samples.iter_mut() {
        *s *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_returns_unity() {
        let g = linear_gain(ReplayGainMode::Off, GainValues { track_db: Some(-3.0), album_db: Some(-6.0) });
        assert_eq!(g, 1.0);
    }

    #[test]
    fn track_mode_uses_track_value() {
        let g = linear_gain(ReplayGainMode::Track, GainValues { track_db: Some(-6.0), album_db: Some(0.0) });
        // -6 dB ≈ 0.501 linear
        assert!((g - 0.5012).abs() < 0.001);
    }

    #[test]
    fn album_mode_falls_back_to_track() {
        let g = linear_gain(ReplayGainMode::Album, GainValues { track_db: Some(-3.0), album_db: None });
        // -3 dB ≈ 0.708 linear
        assert!((g - 0.7079).abs() < 0.001);
    }

    #[test]
    fn missing_values_return_unity() {
        let g = linear_gain(ReplayGainMode::Track, GainValues::default());
        assert_eq!(g, 1.0);
    }

    #[test]
    fn apply_gain_scales_each_sample() {
        let mut samples = [1.0f32, 0.5, -0.5, 0.25];
        apply_gain(&mut samples, 0.5);
        assert!((samples[0] - 0.5).abs() < f32::EPSILON);
        assert!((samples[1] - 0.25).abs() < f32::EPSILON);
        assert!((samples[2] - -0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn unity_gain_is_no_op() {
        let mut samples = [1.0f32, 0.5, -0.25];
        let original = samples;
        apply_gain(&mut samples, 1.0);
        assert_eq!(samples, original);
    }
}
