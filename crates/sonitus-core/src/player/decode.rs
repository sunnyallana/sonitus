//! Symphonia-based audio decoder that pumps f32 samples into a ringbuffer.
//!
//! ## Threading model
//!
//! - **Decode thread**: owns a `DecodeStream`, calls `tick` on every
//!   iteration. Each tick decodes one packet, resamples to the output
//!   rate, conforms channels, and pushes to the ring.
//! - **Output thread** (cpal callback in `output_native.rs`): drains the
//!   ringbuffer via [`SampleSource`].
//!
//! ## Rate + channel conform
//!
//! Files come in at any rate (44.1, 48, 88.2, 96 kHz...) and any channel
//! count (1, 2, 6...). The cpal stream runs at the audio device's *actual*
//! preferred format. The decoder resamples + remixes into that format
//! before writing to the ring; the output thread is then a dumb f32 mover.

use crate::error::{Result, SonitusError};
use crate::player::output_native::SampleSource;
use parking_lot::Mutex;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
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
/// decode and output threads. All samples in the buffer are at the
/// **output** rate + channels, not the file's.
#[derive(Clone, Default)]
pub struct AudioRing {
    inner: Arc<Mutex<RingState>>,
}

struct RingState {
    buffer: VecDeque<f32>,
    /// The format the OUTPUT (cpal) stream is running at — what we have
    /// to feed the device. Set by the engine via `set_output_format`.
    output_rate_hz: u32,
    output_channels: u16,
    /// Frame count at output rate. Used by the engine to derive playback
    /// position from buffered/written samples.
    frames_written: u64,
    /// Linear amplitude multiplier applied to every sample in `fill()`.
    /// Lives here (rather than as a separate atomic) because the cpal
    /// callback already holds the ring's lock while draining — so reading
    /// it costs nothing extra. The engine updates it via `set_volume`.
    volume: f32,
}

impl Default for RingState {
    fn default() -> Self {
        Self {
            buffer: VecDeque::new(),
            output_rate_hz: 0,
            output_channels: 0,
            frames_written: 0,
            volume: 1.0,
        }
    }
}

/// Maximum buffered samples (interleaved, all channels). At 48 kHz stereo
/// that's ~5 seconds.
pub const MAX_BUFFERED_SAMPLES: usize = 48_000 * 2 * 5;

impl AudioRing {
    /// Construct an empty ringbuffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push samples (interleaved, at the output rate + channels). Returns
    /// the number of samples actually written.
    pub fn push(&self, samples: &[f32]) -> usize {
        let mut state = self.inner.lock();
        let space = MAX_BUFFERED_SAMPLES.saturating_sub(state.buffer.len());
        let to_write = space.min(samples.len());
        state.buffer.extend(samples[..to_write].iter().copied());
        let chans = state.output_channels.max(1) as usize;
        state.frames_written += (to_write / chans) as u64;
        to_write
    }

    /// Set the OUTPUT format (cpal stream's rate + channels). The decoder
    /// reads this and configures its resampler/upmixer accordingly.
    pub fn set_output_format(&self, sample_rate_hz: u32, channels: u16) {
        let mut state = self.inner.lock();
        state.output_rate_hz = sample_rate_hz;
        state.output_channels = channels;
    }

    /// Compatibility shim used by older callers (e.g. probe code paths).
    /// Equivalent to `set_output_format`.
    pub fn set_format(&self, sample_rate_hz: u32, channels: u16) {
        self.set_output_format(sample_rate_hz, channels);
    }

    /// Flush any buffered samples (called on Stop / track change).
    pub fn clear(&self) {
        let mut state = self.inner.lock();
        state.buffer.clear();
        state.frames_written = 0;
    }

    /// Mark the producer as finished — for now informational only; the
    /// output thread keeps draining whatever's left.
    pub fn mark_eof(&self) {
        // No-op: output drains naturally as the buffer empties.
    }

    /// Sample rate of the buffered audio (the OUTPUT rate).
    pub fn sample_rate(&self) -> u32 {
        self.inner.lock().output_rate_hz
    }

    /// Channel count of the buffered audio (the OUTPUT channel count).
    pub fn channels(&self) -> u16 {
        self.inner.lock().output_channels
    }

    /// Frames written so far at the output rate. Used by the engine to
    /// derive playback position.
    pub fn frames_written(&self) -> u64 {
        self.inner.lock().frames_written
    }

    /// Buffered samples currently waiting to be played.
    pub fn buffered_samples(&self) -> usize {
        self.inner.lock().buffer.len()
    }

    /// Set the output volume (linear amplitude, `0.0..=1.0`). Applied by
    /// the cpal callback as it drains samples in `fill()`. Changes take
    /// effect on the next callback (~ buffer_size frames of latency at
    /// most), so the volume slider feels responsive.
    pub fn set_volume(&self, amplitude: f32) {
        self.inner.lock().volume = amplitude.clamp(0.0, 1.0);
    }
}

impl SampleSource for AudioRing {
    fn fill(&mut self, out: &mut [f32]) -> usize {
        let mut state = self.inner.lock();
        let vol = state.volume;
        let mut written = 0;
        while written < out.len() {
            match state.buffer.pop_front() {
                Some(s) => {
                    out[written] = s * vol;
                    written += 1;
                }
                None => break,
            }
        }
        written
    }
}

/// One Symphonia decoder + (optional) per-channel rubato resampler.
pub struct DecodeStream {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    /// File's native sample rate.
    pub sample_rate_hz: u32,
    /// File's native channel count.
    pub channels: u16,
    /// Total duration in seconds, if known.
    pub duration_secs: Option<f64>,
    /// Per-track linear gain (from ReplayGain).
    pub gain: f32,
    ring: AudioRing,
    /// The OUTPUT format we're targeting (cpal's actual rate + channels).
    output_rate_hz: u32,
    output_channels: u16,
    /// Resampler per channel (rubato is per-channel). `None` if file rate
    /// already matches output rate.
    resampler: Option<SincFixedIn<f32>>,
    /// De-interleaved buffer of leftover samples per channel that haven't
    /// been resampled yet — rubato wants fixed-size input chunks.
    deint_buf: Vec<Vec<f32>>,
    /// Fixed input chunk size the resampler wants per channel.
    resample_chunk: usize,
    /// Output samples produced but not yet pushed to the ring (because
    /// the ring was full). Drained first on the next tick — keeps
    /// ordering correct and lets `tick` return immediately when the ring
    /// fills, so the engine loop can promptly handle commands like Pause
    /// instead of being blocked in a back-pressure spin.
    pending_out: Vec<f32>,
    finished: bool,
}

impl DecodeStream {
    /// Construct from a local file path.
    pub fn open_file(path: &Path, ring: AudioRing, gain: f32) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| SonitusError::Audio(format!("open: {e}")))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        Self::open_mss(mss, ring, gain, path)
    }

    fn open_mss(
        mss: MediaSourceStream,
        ring: AudioRing,
        gain: f32,
        path_for_hint: &Path,
    ) -> Result<Self> {
        let mut hint = Hint::new();
        if let Some(ext) = path_for_hint.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| SonitusError::Audio(format!("probe: {e}")))?;

        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| SonitusError::Audio("no audio track in file".into()))?;
        let track_id = track.id;
        let cp = &track.codec_params;
        let file_rate = cp.sample_rate.unwrap_or(44_100);
        let file_channels = cp.channels.map(|c| c.count() as u16).unwrap_or(2);
        let cp_n_frames = cp.n_frames;
        let cp_time_base = cp.time_base;
        let cp_start_ts = cp.start_ts;

        let decoder = symphonia::default::get_codecs()
            .make(cp, &DecoderOptions::default())
            .map_err(|e| SonitusError::Audio(format!("make decoder: {e}")))?;

        // Try the cheap path first: codec params at probe time.
        let mut format = format;
        let mut duration_secs = match (cp_n_frames, file_rate) {
            (Some(n), rate) if rate > 0 => Some(n as f64 / rate as f64),
            _ => None,
        };

        // For CBR MP3 without a Xing/Info header (common — esp. YouTube
        // rips), Symphonia leaves `n_frames` as None. Walk through the
        // packet stream to count, then seek back to the start. One-time
        // cost ~50-100ms per MP3; the result is the *exact* duration.
        if duration_secs.is_none() {
            let mut last_ts: u64 = cp_start_ts;
            loop {
                match format.next_packet() {
                    Ok(p) => {
                        if p.track_id() == track_id {
                            last_ts = p.ts().saturating_add(p.dur());
                        }
                    }
                    Err(symphonia::core::errors::Error::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(_) => break,
                }
            }
            // Convert ts to seconds. The codec's time_base maps ts → seconds.
            if let Some(tb) = cp_time_base {
                let total_time = tb.calc_time(last_ts.saturating_sub(cp_start_ts));
                duration_secs = Some(total_time.seconds as f64 + total_time.frac);
            } else if file_rate > 0 {
                // Fall back: assume ts is in samples at file_rate.
                duration_secs = Some(last_ts as f64 / file_rate as f64);
            }
            tracing::info!(
                duration_secs = ?duration_secs,
                "decode: counted duration via packet walk (n_frames was unknown)"
            );
            // Seek back to the start so playback begins at 0.
            let _ = format.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::from(0.0),
                    track_id: Some(track_id),
                },
            );
        }

        // Read the OUTPUT format the engine has stamped on the ring.
        let out_rate = ring.sample_rate().max(1);
        let out_channels = ring.channels().max(1);

        // Build a rubato resampler if rates differ. SincFixedIn uses a
        // fixed input chunk size; we pick 1024 samples per channel as a
        // sane default — enough latency-mass for quality, small enough
        // to keep the ring fed.
        let resample_chunk = 1024;
        let resampler = if file_rate != out_rate {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let r = SincFixedIn::<f32>::new(
                out_rate as f64 / file_rate as f64,
                2.0,
                params,
                resample_chunk,
                file_channels as usize,
            )
            .map_err(|e| SonitusError::Audio(format!("rubato init: {e}")))?;
            tracing::info!(
                file_rate,
                out_rate,
                file_channels,
                out_channels,
                "decode: resampler engaged"
            );
            Some(r)
        } else {
            tracing::info!(
                rate = file_rate,
                file_channels,
                out_channels,
                "decode: no resample needed"
            );
            None
        };

        let deint_buf = (0..file_channels as usize).map(|_| Vec::new()).collect();

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate_hz: file_rate,
            channels: file_channels,
            duration_secs,
            ring,
            gain,
            output_rate_hz: out_rate,
            output_channels: out_channels,
            resampler,
            deint_buf,
            resample_chunk,
            pending_out: Vec::new(),
            finished: false,
        })
    }

    /// Decode + resample + emit one tick's worth of work. **Non-blocking**:
    /// if the ring is full, leftover samples are stashed in `pending_out`
    /// and we return immediately, letting the engine loop process commands
    /// (Pause, Seek, etc.) before calling tick again.
    ///
    /// Returns Ok(true) while there's more to do, Ok(false) on EOF.
    pub fn tick(&mut self) -> Result<bool> {
        // 1. Drain any pending output left over from a prior tick where
        //    the ring was full. Until that's empty, we don't decode more
        //    (preserves audio ordering + bounds memory).
        if !self.pending_out.is_empty() {
            let n = self.ring.push(&self.pending_out);
            self.pending_out.drain(..n);
            if !self.pending_out.is_empty() {
                // Ring still hasn't drained enough; come back later.
                return Ok(true);
            }
        }

        if self.finished {
            return Ok(false);
        }

        let packet = match self.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                self.finished = true;
                self.ring.mark_eof();
                self.flush_resampler();
                return Ok(false);
            }
            Err(e) => return Err(SonitusError::Audio(format!("next_packet: {e}"))),
        };
        if packet.track_id() != self.track_id {
            return Ok(true);
        }

        let decoded = match self.decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => return Ok(true),
            Err(e) => return Err(SonitusError::Audio(format!("decode: {e}"))),
        };

        let spec = *decoded.spec();
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        let interleaved = samples.samples();

        let n_chan = self.channels as usize;
        let frames = interleaved.len() / n_chan.max(1);
        for ch in 0..n_chan {
            self.deint_buf[ch].reserve(frames);
            for f in 0..frames {
                self.deint_buf[ch].push(interleaved[f * n_chan + ch]);
            }
        }

        self.flush_resampler();
        Ok(true)
    }

    /// Push as many full resampler chunks through as possible. Stops as
    /// soon as `pending_out` accumulates anything (ring filled mid-write)
    /// to keep tick latency bounded.
    fn flush_resampler(&mut self) {
        loop {
            if !self.pending_out.is_empty() {
                // Backpressure: don't decode/resample more until prior
                // output drains.
                return;
            }
            let chunk = self.resample_chunk;
            let have_full_chunk = self.deint_buf.iter().all(|c| c.len() >= chunk);
            if !have_full_chunk { break; }

            let processed: Vec<Vec<f32>> = if let Some(rs) = self.resampler.as_mut() {
                let input: Vec<&[f32]> = self.deint_buf.iter().map(|c| &c[..chunk]).collect();
                let mut out = vec![vec![0.0f32; rs.output_frames_max()]; self.channels as usize];
                let (in_used, out_written) = match rs.process_into_buffer(&input, &mut out, None) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "rubato process error");
                        for c in &mut self.deint_buf { c.drain(..chunk); }
                        return;
                    }
                };
                for c in &mut self.deint_buf { c.drain(..in_used); }
                for ch_buf in out.iter_mut() { ch_buf.truncate(out_written); }
                out
            } else {
                let mut out = Vec::with_capacity(self.channels as usize);
                for c in self.deint_buf.iter_mut() {
                    let taken: Vec<f32> = c.drain(..chunk).collect();
                    out.push(taken);
                }
                out
            };

            self.write_interleaved(&processed);
        }
    }

    /// Take per-channel samples, conform to output channel count, interleave,
    /// apply gain, push as much as fits into the ring. Anything that
    /// doesn't fit goes into `pending_out` for next tick to drain.
    fn write_interleaved(&mut self, per_channel: &[Vec<f32>]) {
        if per_channel.is_empty() || per_channel[0].is_empty() {
            return;
        }
        let frames = per_channel[0].len();
        let in_ch = self.channels as usize;
        let out_ch = self.output_channels.max(1) as usize;
        let mut out_buf: Vec<f32> = Vec::with_capacity(frames * out_ch);

        for f in 0..frames {
            for oc in 0..out_ch {
                let v = if in_ch == out_ch {
                    per_channel[oc][f]
                } else if in_ch == 1 {
                    per_channel[0][f]
                } else if in_ch >= 2 && out_ch == 1 {
                    0.5 * (per_channel[0][f] + per_channel[1][f])
                } else if in_ch == 2 && out_ch >= 2 {
                    if oc < 2 { per_channel[oc][f] } else { 0.0 }
                } else if oc < in_ch {
                    per_channel[oc][f]
                } else {
                    0.0
                };
                out_buf.push(v * self.gain);
            }
        }

        let pushed = self.ring.push(&out_buf);
        if pushed < out_buf.len() {
            // Save the rest for next tick. The engine loop sleeps a few
            // ms between ticks, giving cpal time to drain the ring.
            self.pending_out.extend_from_slice(&out_buf[pushed..]);
        }
    }

    /// Seek to a position in seconds.
    pub fn seek_to(&mut self, seconds: f64) -> Result<()> {
        let time = Time::from(seconds);
        self.format
            .seek(SeekMode::Coarse, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| SonitusError::Audio(format!("seek: {e}")))?;
        self.ring.clear();
        for c in &mut self.deint_buf { c.clear(); }
        self.pending_out.clear();
        if let Some(r) = self.resampler.as_mut() { r.reset(); }
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
        ring.set_output_format(48_000, 2);
        ring.push(&[0.5, -0.5, 0.25, -0.25]);
        assert_eq!(ring.buffered_samples(), 4);

        let mut out = [0.0f32; 4];
        let mut sink = ring.clone();
        let n = sink.fill(&mut out);
        assert_eq!(n, 4);
        assert_eq!(out, [0.5, -0.5, 0.25, -0.25]);
    }

    #[test]
    fn ring_caps_at_max_buffered() {
        let ring = AudioRing::new();
        ring.set_output_format(48_000, 2);
        let huge = vec![0.0f32; MAX_BUFFERED_SAMPLES + 10_000];
        let n = ring.push(&huge);
        assert_eq!(n, MAX_BUFFERED_SAMPLES);
    }
}
