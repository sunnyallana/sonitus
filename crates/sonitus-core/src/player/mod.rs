//! Audio playback engine.
//!
//! The player runs on **two dedicated OS threads** (not tokio tasks):
//!
//! 1. **Decode thread** — Symphonia probes/decodes packets, applies
//!    ReplayGain, pushes f32 samples to a ring buffer. Pre-decodes the
//!    next track when the current one has < 10s remaining (gapless).
//! 2. **Output thread** — `cpal` (or web-sys `AudioContext` on WASM) pulls
//!    samples from the ring buffer and feeds the OS audio device.
//!
//! Tokio tasks are unsuitable here because the output callback has
//! real-time deadlines (1-10 ms) that the async runtime can't honor under
//! load. OS threads with ringbuffer-based handoff are the standard pattern.
//!
//! ## Communication
//!
//! Both threads talk to the rest of the app via [`crossbeam_channel`]:
//!
//! - UI / orchestrator → decode thread: [`PlayerCommand`].
//! - Decode thread → UI: [`PlayerEvent`] (Progress, TrackChanged, etc.)
//!
//! ## Web target
//!
//! On `wasm32-unknown-unknown`, `cpal` is unavailable. The `output_web.rs`
//! module substitutes a `web-sys::AudioContext` with an `AudioWorkletNode`
//! sourcing from the same ring buffer (via `SharedArrayBuffer`).

pub mod commands;
pub mod engine;
pub mod events;
pub mod gapless;
pub mod queue;
pub mod replaygain;

#[cfg(not(target_arch = "wasm32"))]
pub mod output_native;

#[cfg(target_arch = "wasm32")]
pub mod output_web;

pub use commands::{PlayerCommand, RepeatMode};
pub use engine::PlayerHandle;
pub use events::PlayerEvent;
pub use queue::PlayQueue;
