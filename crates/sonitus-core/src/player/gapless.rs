//! Gapless pre-buffering of the next track.
//!
//! When the current track has fewer than [`PREBUFFER_THRESHOLD_MS`] ms
//! remaining, the decode thread starts pre-decoding the *next* track into
//! a secondary ring buffer. When the current track ends, we swap buffers —
//! the listener hears no silence between tracks.
//!
//! ## Crossfade
//!
//! If `crossfade_secs > 0`, instead of a hard cut at end-of-track we mix
//! the tail of N with the head of N+1 over `crossfade_secs` seconds.
//! Crossfade is implemented as a linear envelope on each buffer:
//! the outgoing track ramps `1.0 → 0.0`, the incoming `0.0 → 1.0`, and we
//! sum the two streams.

/// Begin pre-decoding the next track when the current track has this
/// many ms or fewer remaining.
pub const PREBUFFER_THRESHOLD_MS: u64 = 10_000;

/// Linear-fade envelope value at a given position within a fade.
///
/// `pos_in_fade` is `0.0..=1.0` from start of fade. Returns the multiplier
/// to apply to the *outgoing* track's samples (1.0 → 0.0).
pub fn fade_out_envelope(pos_in_fade: f32) -> f32 {
    1.0 - pos_in_fade.clamp(0.0, 1.0)
}

/// Linear-fade envelope value at a given position within a fade.
/// Returns the multiplier to apply to the *incoming* track's samples
/// (0.0 → 1.0).
pub fn fade_in_envelope(pos_in_fade: f32) -> f32 {
    pos_in_fade.clamp(0.0, 1.0)
}

/// Crossfade two streams of equal length, sample-for-sample.
/// `out` and `in_` must be the same length; outputs interleaved into `dest`.
/// `crossfade_progress` runs from 0.0 (start) to 1.0 (end) across the buffer.
pub fn crossfade_into(out: &[f32], in_: &[f32], dest: &mut [f32], start_progress: f32, end_progress: f32) {
    debug_assert_eq!(out.len(), in_.len());
    debug_assert_eq!(out.len(), dest.len());
    let n = out.len() as f32;
    if n == 0.0 { return; }
    let span = end_progress - start_progress;
    for (i, slot) in dest.iter_mut().enumerate() {
        let p = start_progress + span * (i as f32 / n);
        let a = fade_out_envelope(p);
        let b = fade_in_envelope(p);
        *slot = out[i] * a + in_[i] * b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_out_starts_at_one_ends_at_zero() {
        assert!((fade_out_envelope(0.0) - 1.0).abs() < f32::EPSILON);
        assert!(fade_out_envelope(1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fade_in_starts_at_zero_ends_at_one() {
        assert!(fade_in_envelope(0.0).abs() < f32::EPSILON);
        assert!((fade_in_envelope(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fades_clamp_outside_range() {
        assert_eq!(fade_out_envelope(-0.1), 1.0);
        assert_eq!(fade_out_envelope(1.5), 0.0);
        assert_eq!(fade_in_envelope(-0.1), 0.0);
        assert_eq!(fade_in_envelope(1.5), 1.0);
    }

    #[test]
    fn crossfade_at_start_is_outgoing_only() {
        let out = vec![1.0, 1.0, 1.0, 1.0];
        let in_ = vec![0.5, 0.5, 0.5, 0.5];
        let mut dest = vec![0.0; 4];
        // start_progress = 0.0 means outgoing weight is 1.0.
        crossfade_into(&out, &in_, &mut dest, 0.0, 0.0);
        for d in dest { assert!((d - 1.0).abs() < 0.01); }
    }

    #[test]
    fn crossfade_at_end_is_incoming_only() {
        let out = vec![1.0, 1.0, 1.0, 1.0];
        let in_ = vec![0.5, 0.5, 0.5, 0.5];
        let mut dest = vec![0.0; 4];
        // end_progress = 1.0 means incoming weight at end is 1.0.
        crossfade_into(&out, &in_, &mut dest, 1.0, 1.0);
        for d in dest { assert!((d - 0.5).abs() < 0.01); }
    }
}
