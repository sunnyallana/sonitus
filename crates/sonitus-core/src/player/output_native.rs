//! Native (desktop / mobile) audio output via `cpal`.
//!
//! This module is excluded from `wasm32` builds — see [`output_web`] for
//! the browser equivalent.
//!
//! ## Lifecycle
//!
//! ```ignore
//!   let output = NativeOutput::default()?;
//!   let stream = output.start(reader)?;  // begins playing
//!   // ... stream is dropped on Stop
//! ```
//!
//! [`output_web`]: super::output_web

#![cfg(not(target_arch = "wasm32"))]

use crate::error::{Result, SonitusError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};

/// Source of f32 audio frames the output stream pulls from.
pub trait SampleSource: Send + 'static {
    /// Fill `out` with samples and return how many frames were written.
    /// `out` is interleaved (`samples = frames * channels`).
    fn fill(&mut self, out: &mut [f32]) -> usize;
}

/// Owner of a `cpal::Stream`. Drop to stop playback.
pub struct NativeOutput {
    /// Held to keep the audio stream alive. Dropping stops playback.
    /// Used directly for pause()/resume() so audio stops at the device
    /// level rather than waiting for the 5-second ring buffer to drain.
    stream: Stream,
    /// Selected device name for the engine to report back.
    pub device_name: String,
    /// The sample rate cpal chose for the stream.
    pub sample_rate_hz: u32,
    /// Number of channels in the stream.
    pub channels: u16,
}

impl NativeOutput {
    /// Stop the cpal stream from pulling samples. Audio output goes
    /// silent immediately; existing buffered samples stay in the ring
    /// for resume().
    pub fn pause(&self) {
        if let Err(e) = self.stream.pause() {
            tracing::warn!(error = %e, "cpal stream.pause() failed");
        }
    }

    /// Resume the cpal stream after pause().
    pub fn resume(&self) {
        if let Err(e) = self.stream.play() {
            tracing::warn!(error = %e, "cpal stream.play() failed");
        }
    }
}

impl NativeOutput {
    /// Enumerate available output devices on the default host.
    pub fn list_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let mut names = Vec::new();
        let devices = host
            .output_devices()
            .map_err(|e| SonitusError::AudioOutput(e.to_string()))?;
        for dev in devices {
            if let Ok(n) = dev.name() {
                names.push(n);
            }
        }
        Ok(names)
    }

    /// Open the default output device and start a stream at the device's
    /// preferred format. The decoder is responsible for resampling its
    /// output to match `sample_rate_hz` and `channels` (see `DecodeStream`).
    pub fn start_default<S: SampleSource>(mut source: S) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| SonitusError::AudioOutput("no default output device".into()))?;
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());

        // Negotiate at the device's true preferred format — the OS will
        // never lie about this. Then the decoder resamples to match.
        // (Earlier we tried to *request* the file's rate and let WASAPI
        // resample; that caused unreliable behavior on shared-mode where
        // Windows silently substitutes the default rate.)
        let supported = device
            .default_output_config()
            .map_err(|e| SonitusError::AudioOutput(e.to_string()))?;
        let sample_format = supported.sample_format();
        let stream_config: StreamConfig = supported.into();
        let sample_rate_hz = stream_config.sample_rate.0;
        let channels = stream_config.channels;

        let err_fn = |e| tracing::warn!("audio stream error: {e}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    let n = source.fill(data);
                    if n < data.len() {
                        // Underrun: silence the rest. Do NOT panic in the
                        // audio callback; the OS will hate you.
                        for s in &mut data[n..] {
                            *s = 0.0;
                        }
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    let mut tmp = vec![0.0f32; data.len()];
                    let n = source.fill(&mut tmp);
                    for (i, s) in data.iter_mut().enumerate() {
                        let v = if i < n { tmp[i] } else { 0.0 };
                        *s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    let mut tmp = vec![0.0f32; data.len()];
                    let n = source.fill(&mut tmp);
                    for (i, s) in data.iter_mut().enumerate() {
                        let v = if i < n { tmp[i] } else { 0.0 };
                        *s = ((v.clamp(-1.0, 1.0) + 1.0) * 32767.5) as u16;
                    }
                },
                err_fn,
                None,
            ),
            _ => return Err(SonitusError::AudioOutput("unsupported sample format".into())),
        }
        .map_err(|e| SonitusError::AudioOutput(e.to_string()))?;

        stream
            .play()
            .map_err(|e| SonitusError::AudioOutput(e.to_string()))?;

        Ok(Self {
            stream,
            device_name,
            sample_rate_hz,
            channels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_devices_returns_a_list_or_empty() {
        // Some CI runners have no audio at all; we just check it doesn't panic.
        let _ = NativeOutput::list_devices();
    }
}
