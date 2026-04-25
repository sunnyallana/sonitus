//! Decoder + ringbuffer between the decode and output threads.
//!
//! ## Status
//!
//! The Symphonia 0.6 alpha.2 API differs significantly from alpha.1
//! (audio codecs moved into `codecs::audio`, `Decoder` renamed to
//! `AudioDecoder`, `SampleBuffer` replaced by `AudioBuffer<S>`,
//! `Probe::format()` lifetime-parameterized). The full migration is
//! tracked separately; this module currently provides:
//!
//! - The `AudioRing` SPSC-style buffer (production-ready).
//! - `DecodeStream::open_file` returns a typed error so the engine
//!   surfaces "decoder not yet wired" through `PlayerEvent::Error`
//!   without crashing.
//!
//! Once the Symphonia 0.6 migration lands, only the body of
//! `DecodeStream::open_file` and `tick`/`seek_to` need to change — the
//! ring API stays stable.

use crate::error::{Result, SonitusError};
use crate::player::output_native::SampleSource;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

/// Lock-protected `VecDeque<f32>` used as a ringbuffer between the
/// decode and output threads.
///
/// Mutex chosen over a true lock-free SPSC queue for simplicity:
/// `parking_lot::Mutex` is a single atomic compare-exchange (~5 ns)
/// when uncontended, well within the audio callback's budget.
#[derive(Clone, Default)]
pub struct AudioRing {
    inner: Arc<Mutex<RingState>>,
}

#[derive(Default)]
struct RingState {
    buffer: VecDeque<f32>,
    sample_rate_hz: u32,
    channels: u16,
    eof: bool,
    frames_written: u64,
}

/// Maximum buffered samples (interleaved, all channels). At 48 kHz stereo
/// that's ~5 seconds — enough to absorb decode jitter.
pub const MAX_BUFFERED_SAMPLES: usize = 48_000 * 2 * 5;

impl AudioRing {
    /// Construct an empty ringbuffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push samples produced by the decoder. Returns the number actually
    /// written (less than `samples.len()` if the buffer hit capacity).
    pub fn push(&self, samples: &[f32]) -> usize {
        let mut state = self.inner.lock();
        let space = MAX_BUFFERED_SAMPLES.saturating_sub(state.buffer.len());
        let to_write = space.min(samples.len());
        state.buffer.extend(samples[..to_write].iter().copied());
        let chans = state.channels.max(1) as usize;
        state.frames_written += (to_write / chans) as u64;
        to_write
    }

    /// Configure stream parameters before/after probing. Caller: decode thread.
    pub fn set_format(&self, sample_rate_hz: u32, channels: u16) {
        let mut state = self.inner.lock();
        state.sample_rate_hz = sample_rate_hz;
        state.channels = channels;
    }

    /// Flush any buffered samples (called on Stop / track change).
    pub fn clear(&self) {
        let mut state = self.inner.lock();
        state.buffer.clear();
        state.eof = false;
        state.frames_written = 0;
    }

    /// Mark the producer as finished. The output thread keeps draining
    /// remaining samples; once empty + EOF, playback ends.
    pub fn mark_eof(&self) {
        self.inner.lock().eof = true;
    }

    /// Sample rate currently in use.
    pub fn sample_rate(&self) -> u32 {
        self.inner.lock().sample_rate_hz
    }

    /// Channel count currently in use.
    pub fn channels(&self) -> u16 {
        self.inner.lock().channels
    }

    /// Frames written so far.
    pub fn frames_written(&self) -> u64 {
        self.inner.lock().frames_written
    }

    /// Buffered samples currently waiting to be played.
    pub fn buffered_samples(&self) -> usize {
        self.inner.lock().buffer.len()
    }
}

impl SampleSource for AudioRing {
    fn fill(&mut self, out: &mut [f32]) -> usize {
        let mut state = self.inner.lock();
        let mut written = 0;
        while written < out.len() {
            match state.buffer.pop_front() {
                Some(s) => {
                    out[written] = s;
                    written += 1;
                }
                None => break,
            }
        }
        written
    }
}

/// Symphonia-backed decoder that pumps f32 samples into the ring.
///
/// The current build returns an explicit error from `open_file` while the
/// migration to Symphonia 0.6 alpha.2's reorganized API is in progress.
/// All other engine plumbing (queue, gapless math, ring, output) is live —
/// when this method starts returning `Ok`, audio plays end to end.
pub struct DecodeStream {
    /// The negotiated sample rate.
    pub sample_rate_hz: u32,
    /// The negotiated channel count.
    pub channels: u16,
    /// Total duration in seconds, if available.
    pub duration_secs: Option<f64>,
    /// Per-track linear gain factor (from ReplayGain).
    pub gain: f32,
    ring: AudioRing,
    finished: bool,
}

impl DecodeStream {
    /// Construct from a local file path.
    ///
    /// Currently returns `Err` until the Symphonia 0.6 alpha.2 migration
    /// completes. The engine handles this gracefully: it emits a
    /// `PlayerEvent::Error` and stays alive for the next track.
    pub fn open_file(_path: &Path, ring: AudioRing, gain: f32) -> Result<Self> {
        let _ = (ring, gain);
        Err(SonitusError::Audio(
            "audio decode is pending migration to Symphonia 0.6 alpha.2 API \
             (the rest of the engine — queue, ring, cpal output — is live)"
                .into(),
        ))
    }

    /// Decode and push the next packet. Returns `Ok(true)` if there's
    /// more data, `Ok(false)` if we hit end of stream.
    pub fn tick(&mut self) -> Result<bool> {
        if self.finished {
            return Ok(false);
        }
        // No-op until open_file is wired.
        self.finished = true;
        self.ring.mark_eof();
        Ok(false)
    }

    /// Seek to a position in seconds.
    pub fn seek_to(&mut self, _seconds: f64) -> Result<()> {
        self.ring.clear();
        self.ring.set_format(self.sample_rate_hz, self.channels);
        self.finished = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_pushes_then_drains() {
        let ring = AudioRing::new();
        ring.set_format(48_000, 2);
        ring.push(&[0.5, -0.5, 0.25, -0.25]);
        assert_eq!(ring.buffered_samples(), 4);

        let mut out = [0.0f32; 4];
        let mut sink = ring.clone();
        let n = sink.fill(&mut out);
        assert_eq!(n, 4);
        assert_eq!(out, [0.5, -0.5, 0.25, -0.25]);
        assert_eq!(ring.buffered_samples(), 0);
    }

    #[test]
    fn ring_caps_at_max_buffered() {
        let ring = AudioRing::new();
        ring.set_format(48_000, 2);
        let huge = vec![0.0f32; MAX_BUFFERED_SAMPLES + 10_000];
        let n = ring.push(&huge);
        assert_eq!(n, MAX_BUFFERED_SAMPLES);
        assert_eq!(ring.buffered_samples(), MAX_BUFFERED_SAMPLES);
    }

    #[test]
    fn ring_underrun_fills_zero_unwritten() {
        let ring = AudioRing::new();
        ring.set_format(48_000, 2);
        ring.push(&[1.0, 2.0]);
        let mut out = [0.0f32; 5];
        let mut sink = ring.clone();
        let n = sink.fill(&mut out);
        assert_eq!(n, 2);
        assert_eq!(&out[..2], &[1.0, 2.0]);
    }

    #[test]
    fn open_file_returns_clear_pending_error() {
        let r = DecodeStream::open_file(Path::new("/nope.mp3"), AudioRing::new(), 1.0);
        assert!(matches!(r, Err(SonitusError::Audio(_))));
    }
}
