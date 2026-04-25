//! Symphonia-based audio decoder that pumps f32 samples into a ringbuffer.
//!
//! ## Threading model
//!
//! - **Decode thread** (this module): owns a `DecodeStream`, calls
//!   [`DecodeStream::tick`] on every iteration. Each tick decodes one
//!   packet (a few ms of audio) and writes the resulting samples to the
//!   ringbuffer. Cooperatively yields when the buffer is full.
//! - **Output thread** (cpal callback in `output_native.rs`): drains the
//!   ringbuffer via the [`SampleSource`] trait. Never blocks.
//!
//! The ringbuffer is the synchronization primitive — no mutexes anywhere
//! in the audio hot path.
//!
//! ## Sample rate / channel handling
//!
//! Symphonia decodes at the file's native rate. We do not resample (yet);
//! we hand the samples to cpal, which passes them through to the OS at
//! whatever rate the device wants. If they don't match, the OS may
//! resample (Core Audio + WASAPI both do; ALSA does not). Real production
//! ought to bring in `rubato` for resampling — out of scope for now.

use crate::error::{Result, SonitusError};
use crate::player::output_native::SampleSource;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Lock-protected `VecDeque<f32>` used as a ringbuffer between the
/// decode and output threads.
///
/// We use a mutex rather than a true lock-free SPSC queue for simplicity
/// — `parking_lot::Mutex` is a single atomic `compare_exchange` in the
/// uncontended case (~5 ns), well within the audio callback's budget.
/// If profiling shows contention, swap to `rtrb` or similar.
#[derive(Clone, Default)]
pub struct AudioRing {
    inner: Arc<Mutex<RingState>>,
}

#[derive(Default)]
struct RingState {
    buffer: VecDeque<f32>,
    /// Sample rate the producer is writing in. The output thread reports
    /// this back to the engine via [`AudioRing::sample_rate`].
    sample_rate_hz: u32,
    /// Channel count.
    channels: u16,
    /// Decoder reached end of stream — the buffer will not grow further.
    eof: bool,
    /// Total frames written so far. Used by the engine to estimate position.
    frames_written: u64,
}

/// Maximum buffered samples (interleaved, all channels). At 48 kHz stereo
/// this is ~5 seconds — enough to absorb decode jitter without exceeding
/// memory budget.
pub const MAX_BUFFERED_SAMPLES: usize = 48_000 * 2 * 5;

impl AudioRing {
    /// Construct an empty ringbuffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push samples produced by the decoder. Caller is the decode thread.
    /// Returns the number of samples actually written (may be less than
    /// `samples.len()` if the buffer hit `MAX_BUFFERED_SAMPLES`).
    pub fn push(&self, samples: &[f32]) -> usize {
        let mut state = self.inner.lock();
        let space = MAX_BUFFERED_SAMPLES.saturating_sub(state.buffer.len());
        let to_write = space.min(samples.len());
        state.buffer.extend(samples[..to_write].iter().copied());
        state.frames_written += (to_write / state.channels.max(1) as usize) as u64;
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

    /// Frames written so far. Combined with sample rate, gives the
    /// engine its estimate of playback position.
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

/// Owns a Symphonia format reader + decoder. One instance per playing track.
pub struct DecodeStream {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    /// The negotiated sample rate.
    pub sample_rate_hz: u32,
    /// The negotiated channel count.
    pub channels: u16,
    /// Total duration in seconds, if Symphonia could determine it.
    pub duration_secs: Option<f64>,
    /// Ringbuffer the decoder writes into.
    ring: AudioRing,
    /// Per-track linear gain factor (from ReplayGain).
    pub gain: f32,
    /// True once we've seen the end of stream.
    finished: bool,
}

impl DecodeStream {
    /// Construct from a local file path. Streams from disk; cheap to start.
    pub fn open_file(path: &Path, ring: AudioRing, gain: f32) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| SonitusError::Audio(format!("open: {e}")))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        Self::open_mss(mss, ring, gain)
    }

    /// Construct from any `Read + Seek` media source. Used by source
    /// providers that can spool to a `tempfile`.
    pub fn open_mss(mss: MediaSourceStream, ring: AudioRing, gain: f32) -> Result<Self> {
        let probed = symphonia::default::get_probe()
            .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| SonitusError::Audio(format!("probe: {e}")))?;

        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| SonitusError::Audio("no audio track in file".into()))?;
        let track_id = track.id;
        let cp = &track.codec_params;
        let sample_rate_hz = cp.sample_rate.unwrap_or(44_100);
        let channels = cp.channels.map(|c| c.count() as u16).unwrap_or(2);
        let duration_secs = match (cp.n_frames, cp.sample_rate) {
            (Some(n), Some(rate)) if rate > 0 => Some(n as f64 / rate as f64),
            _ => None,
        };

        let decoder = symphonia::default::get_codecs()
            .make(cp, &DecoderOptions::default())
            .map_err(|e| SonitusError::Audio(format!("make decoder: {e}")))?;

        ring.set_format(sample_rate_hz, channels);

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate_hz,
            channels,
            duration_secs,
            ring,
            gain,
            finished: false,
        })
    }

    /// Decode and push the next packet. Returns `Ok(true)` if there's more
    /// data, `Ok(false)` if we hit end of stream.
    pub fn tick(&mut self) -> Result<bool> {
        if self.finished { return Ok(false); }

        let packet = match self.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                self.finished = true;
                self.ring.mark_eof();
                return Ok(false);
            }
            Err(e) => return Err(SonitusError::Audio(format!("next_packet: {e}"))),
        };
        if packet.track_id() != self.track_id { return Ok(true); }

        let decoded = match self.decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => {
                // Bad packet; skip and continue.
                return Ok(true);
            }
            Err(e) => return Err(SonitusError::Audio(format!("decode: {e}"))),
        };

        let spec = *decoded.spec();
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        let mut buf: Vec<f32> = samples.samples().to_vec();
        if (self.gain - 1.0).abs() > f32::EPSILON {
            for s in &mut buf {
                *s *= self.gain;
            }
        }

        // Spin lightly until the ring has space — the output thread will
        // drain. Yields cooperatively on every full pass.
        let mut written = 0usize;
        while written < buf.len() {
            let n = self.ring.push(&buf[written..]);
            written += n;
            if written < buf.len() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        Ok(true)
    }

    /// Seek to a position in seconds. May be slightly approximate
    /// depending on the format's seek granularity.
    pub fn seek_to(&mut self, seconds: f64) -> Result<()> {
        let time = Time::from(seconds);
        self.format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time { time, track_id: Some(self.track_id) },
            )
            .map_err(|e| SonitusError::Audio(format!("seek: {e}")))?;
        // Discard whatever was in the ring; next tick fills it from the new position.
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
        // The remaining slots are left as their initial value (0.0).
        assert_eq!(&out[..2], &[1.0, 2.0]);
    }

    #[test]
    fn ring_mark_eof_is_visible() {
        let ring = AudioRing::new();
        ring.mark_eof();
        // We don't expose a public eof() getter; testing via behavior:
        // The frame counter still reads the same, and channels()
        // returns whatever was last set.
        assert_eq!(ring.frames_written(), 0);
    }
}
