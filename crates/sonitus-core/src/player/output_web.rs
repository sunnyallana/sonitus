//! Web (`wasm32`) audio output via `web-sys::AudioContext`.
//!
//! Compiled only for `wasm32-unknown-unknown`. On the browser, `cpal`
//! is unavailable, so we use the Web Audio API directly.
//!
//! ## Approach
//!
//! - Create a `web_sys::AudioContext`.
//! - Construct an `AudioBuffer` of f32 samples per chunk and play it via
//!   an `AudioBufferSourceNode`.
//! - For continuous playback, schedule overlapping buffers to avoid
//!   gaps. This gives us coarse control with no need for an
//!   `AudioWorkletNode` (which would require a separate JS bundle).
//!
//! On modern browsers we will eventually upgrade to AudioWorklet for
//! tighter latency, but this implementation is sufficient for the MVP.

#![cfg(target_arch = "wasm32")]

use crate::error::{Result, SonitusError};
use wasm_bindgen::JsCast;

/// Trait satisfied by sample sources usable on the web.
pub trait SampleSource: 'static {
    /// Fill `out` with up to `out.len()` samples; return how many were written.
    fn fill(&mut self, out: &mut [f32]) -> usize;
}

/// Web-side audio output. Holds the AudioContext alive.
pub struct WebOutput {
    ctx: web_sys::AudioContext,
    /// The sample rate the AudioContext is running at (browser-decided).
    pub sample_rate_hz: f32,
    /// Channels (always 2 on the web; mono sources are upmixed by the API).
    pub channels: u16,
}

impl WebOutput {
    /// Construct a new web audio output. Must be called in response to a
    /// user gesture on most browsers (autoplay restrictions); the caller
    /// is responsible for that.
    pub fn new() -> Result<Self> {
        let ctx = web_sys::AudioContext::new()
            .map_err(|e| SonitusError::AudioOutput(format!("AudioContext: {e:?}")))?;
        let sample_rate_hz = ctx.sample_rate();
        Ok(Self {
            ctx,
            sample_rate_hz,
            channels: 2,
        })
    }

    /// Play one chunk: copy samples into an AudioBuffer and start it.
    /// Returns the time (seconds, AudioContext-relative) at which the buffer
    /// will end — caller can schedule the next chunk at that time.
    pub fn play_chunk(&self, samples: &[f32], when_secs: f64) -> Result<f64> {
        let frames = (samples.len() / self.channels as usize).max(1);
        let buffer = self
            .ctx
            .create_buffer(self.channels as u32, frames as u32, self.sample_rate_hz)
            .map_err(|e| SonitusError::AudioOutput(format!("create_buffer: {e:?}")))?;

        // De-interleave into channel-separated buffers.
        for ch in 0..self.channels {
            let mut chan = vec![0.0f32; frames];
            for (i, slot) in chan.iter_mut().enumerate() {
                let sample_idx = i * self.channels as usize + ch as usize;
                if sample_idx < samples.len() {
                    *slot = samples[sample_idx];
                }
            }
            buffer
                .copy_to_channel(&chan, ch as i32)
                .map_err(|e| SonitusError::AudioOutput(format!("copy_to_channel: {e:?}")))?;
        }

        let src = self
            .ctx
            .create_buffer_source()
            .map_err(|e| SonitusError::AudioOutput(format!("create_buffer_source: {e:?}")))?;
        src.set_buffer(Some(&buffer));
        let dest = self.ctx.destination();
        src.connect_with_audio_node(&dest)
            .map_err(|e| SonitusError::AudioOutput(format!("connect: {e:?}")))?;
        src.start_with_when(when_secs)
            .map_err(|e| SonitusError::AudioOutput(format!("start: {e:?}")))?;
        Ok(when_secs + frames as f64 / self.sample_rate_hz as f64)
    }

    /// Suspend the AudioContext (called on Pause for power efficiency).
    pub fn suspend(&self) {
        let _ = self.ctx.suspend();
    }

    /// Resume from suspended state.
    pub fn resume(&self) {
        let _ = self.ctx.resume();
    }
}
