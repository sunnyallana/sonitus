//! Player engine — the decode thread and its public handle.
//!
//! Architecture:
//!
//! ```text
//!   PlayerHandle (cheap clone)
//!         │
//!         │  send(PlayerCommand)
//!         ▼
//!   ┌─────────────────────┐
//!   │  Decode thread      │       crossbeam_channel
//!   │  ───────────────    │       ──────────────────
//!   │  - Symphonia probe  │ ───►  Sender<PlayerEvent>
//!   │  - Decode → f32     │
//!   │  - ReplayGain       │
//!   │  - Push to RB       │
//!   └─────────┬───────────┘
//!             │
//!     ringbuffer (lock-free-ish)
//!             │
//!             ▼
//!   ┌─────────────────────┐
//!   │  Output thread      │
//!   │  ───────────────    │
//!   │  cpal callback pulls│
//!   │  RB → device        │
//!   └─────────────────────┘
//! ```
//!
//! On WASM, the output thread is replaced by `output_web::AudioContextOutput`.

use crate::config::ReplayGainMode;
use crate::error::{Result, SonitusError};
use crate::library::Track;
use crate::player::commands::{PlayerCommand, ReplayGainCommand};
use crate::player::events::PlayerEvent;
use crate::player::queue::PlayQueue;
use crate::player::replaygain::{GainValues, linear_gain};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::player::decode::{AudioRing, DecodeStream};
#[cfg(not(target_arch = "wasm32"))]
use crate::player::output_native::NativeOutput;

/// Cheap clonable handle to the player engine.
///
/// Cloning a handle just clones the underlying channel `Sender` — the
/// decode thread is shared. Drop all clones to let the thread exit.
#[derive(Clone)]
pub struct PlayerHandle {
    cmd_tx: Sender<PlayerCommand>,
    event_rx: Receiver<PlayerEvent>,
}

impl PlayerHandle {
    /// Send a command to the decode thread. Non-blocking; returns an error
    /// only if the engine has shut down.
    pub fn send(&self, cmd: PlayerCommand) -> Result<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| SonitusError::AudioOutput(format!("player engine closed: {e}")))
    }

    /// Receive the next event, blocking. Returns `None` if the engine
    /// has shut down.
    pub fn next_event(&self) -> Option<PlayerEvent> {
        self.event_rx.recv().ok()
    }

    /// Try to receive an event without blocking.
    pub fn try_next_event(&self) -> Option<PlayerEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Borrow the underlying event receiver. Useful if you want to
    /// `select!` on it alongside other channels.
    pub fn event_receiver(&self) -> &Receiver<PlayerEvent> {
        &self.event_rx
    }
}

/// Trait implemented by the orchestrator that supplies the decode thread
/// with bytes for a track. The decode thread doesn't talk to source
/// providers directly — it asks the orchestrator, which knows which source
/// owns each track.
pub trait TrackResolver: Send + Sync + 'static {
    /// Look up a track by ID and return both its DB row and a path to a
    /// playable source (local file or downloaded cache).
    fn resolve(&self, track_id: &str) -> Result<(Track, std::path::PathBuf)>;
}

/// Spawn the decode thread and return a handle.
///
/// `resolver` is called from the decode thread to obtain a local file
/// path for each track. The expectation is that the orchestrator has
/// already streamed/downloaded the file to the offline cache before
/// sending `PlayerCommand::Play`.
pub fn spawn(resolver: Arc<dyn TrackResolver>) -> PlayerHandle {
    let (cmd_tx, cmd_rx) = unbounded::<PlayerCommand>();
    let (evt_tx, evt_rx) = bounded::<PlayerEvent>(256);

    std::thread::Builder::new()
        .name("sonitus-decode".into())
        .spawn(move || {
            let mut state = EngineState::new(evt_tx, resolver);
            state.run(cmd_rx);
        })
        .expect("failed to spawn decode thread");

    PlayerHandle { cmd_tx, event_rx: evt_rx }
}

/// Internal engine state.
struct EngineState {
    queue: PlayQueue,
    volume: f32,
    replay_gain: ReplayGainMode,
    output_device: Option<String>,
    paused: bool,
    /// Duration of the current track in milliseconds.
    duration_ms: u64,
    /// Position in ms at which the ring was last started/cleared. This
    /// gets added to `frames_played / rate` to derive the absolute
    /// position. Set to 0 on a fresh play, set to the seek target on Seek.
    /// Without this, every Seek would visually reset position to 0:00
    /// because we clear the ring (and its frames_written counter).
    seek_offset_ms: u64,
    evt: Sender<PlayerEvent>,
    resolver: Arc<dyn TrackResolver>,
    /// Current track being played (for emitting events).
    current_track: Option<Track>,

    // Audio backend — gated to non-wasm builds.
    #[cfg(not(target_arch = "wasm32"))]
    ring: AudioRing,
    #[cfg(not(target_arch = "wasm32"))]
    output: Option<NativeOutput>,
    #[cfg(not(target_arch = "wasm32"))]
    decoder: Option<DecodeStream>,
}

impl EngineState {
    fn new(evt: Sender<PlayerEvent>, resolver: Arc<dyn TrackResolver>) -> Self {
        Self {
            queue: PlayQueue::new(),
            volume: 1.0,
            replay_gain: ReplayGainMode::Track,
            output_device: None,
            paused: false,
            duration_ms: 0,
            seek_offset_ms: 0,
            evt,
            resolver,
            current_track: None,
            #[cfg(not(target_arch = "wasm32"))]
            ring: AudioRing::new(),
            #[cfg(not(target_arch = "wasm32"))]
            output: None,
            #[cfg(not(target_arch = "wasm32"))]
            decoder: None,
        }
    }

    fn run(&mut self, cmd_rx: Receiver<PlayerCommand>) {
        let mut last_progress_emit = std::time::Instant::now();

        loop {
            // 1. Drain any pending commands non-blockingly.
            while let Ok(cmd) = cmd_rx.try_recv() {
                if matches!(cmd, PlayerCommand::Shutdown) {
                    let _ = self.evt.send(PlayerEvent::Stopped);
                    return;
                }
                self.handle_command(cmd);
            }

            // 2. Drive the decoder if we have one and aren't paused.
            #[cfg(not(target_arch = "wasm32"))]
            {
                if !self.paused {
                    if let Some(decoder) = self.decoder.as_mut() {
                        match decoder.tick() {
                            Ok(true) => { /* keep going */ }
                            Ok(false) => self.on_track_ended(),
                            Err(e) => {
                                let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
                                self.decoder = None;
                            }
                        }
                    }
                }
            }

            // 3. Emit a Progress event ~10 Hz.
            if !self.paused && self.current_track.is_some()
                && last_progress_emit.elapsed() >= std::time::Duration::from_millis(100)
            {
                let position_ms = self.position_ms();
                let buffered = self.buffered_ms();
                let _ = self.evt.send(PlayerEvent::Progress {
                    position_ms,
                    duration_ms: self.duration_ms,
                    buffered_ms: buffered,
                });
                last_progress_emit = std::time::Instant::now();
            }

            // 4. If the decoder is None and the buffer is empty, sleep
            //    longer; otherwise yield briefly so we don't busy-loop.
            #[cfg(not(target_arch = "wasm32"))]
            {
                let idle = self.decoder.is_none() || self.paused;
                if idle {
                    // Block until the next command arrives, but cap at
                    // 100 ms so we still emit progress.
                    if let Ok(cmd) = cmd_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        if matches!(cmd, PlayerCommand::Shutdown) {
                            let _ = self.evt.send(PlayerEvent::Stopped);
                            return;
                        }
                        self.handle_command(cmd);
                    }
                } else {
                    // Active decode: short sleep keeps the buffer flowing
                    // without spinning a core.
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }

            // wasm fallback: same loop body without the audio backend.
            #[cfg(target_arch = "wasm32")]
            {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    /// Position in ms relative to the start of the track.
    ///
    /// = `seek_offset_ms` + (frames_played / output_rate) * 1000
    ///
    /// `frames_played` is the number of frames that have actually left
    /// the ring (decoded - still-buffered). The offset is needed because
    /// every seek calls `ring.clear()`, resetting the ring's
    /// `frames_written` counter to zero — without an explicit offset the
    /// reported position would jump back to 0:00 after each seek.
    fn position_ms(&self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let rate = self.ring.sample_rate().max(1) as u64;
            let channels = self.ring.channels().max(1) as u64;
            let written = self.ring.frames_written();
            let buffered_samples = self.ring.buffered_samples() as u64;
            let buffered_frames = buffered_samples / channels;
            let played_frames = written.saturating_sub(buffered_frames);
            let elapsed_since_seek_ms = played_frames.saturating_mul(1000) / rate;
            self.seek_offset_ms.saturating_add(elapsed_since_seek_ms)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.seek_offset_ms
        }
    }

    fn buffered_ms(&self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let rate = self.ring.sample_rate().max(1) as u64;
            let channels = self.ring.channels().max(1) as u64;
            let buffered = self.ring.buffered_samples() as u64;
            (buffered / channels).saturating_mul(1000) / rate
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    }

    fn handle_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::Play { track_id } => self.play_track(&track_id),
            PlayerCommand::PlayUrl { url: _ } => {
                let _ = self.evt.send(PlayerEvent::Error {
                    message: "PlayUrl: stream-from-URL is handled by the orchestrator (download to cache then Play)".into(),
                });
            }
            PlayerCommand::Pause => {
                self.paused = true;
                // Pause the cpal stream itself so audio stops *now*. If
                // we only set self.paused = true, the device keeps
                // pulling from the ring (~5s of buffered audio) before
                // going silent — that's why pause felt slow.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(out) = &self.output {
                    out.pause();
                }
                let pos = self.position_ms();
                let _ = self.evt.send(PlayerEvent::Paused { position_ms: pos });
            }
            PlayerCommand::Resume => {
                self.paused = false;
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(out) = &self.output {
                    out.resume();
                }
                let pos = self.position_ms();
                let _ = self.evt.send(PlayerEvent::Resumed { position_ms: pos });
            }
            PlayerCommand::Stop => {
                self.paused = false;
                self.current_track = None;
                self.duration_ms = 0;
                self.seek_offset_ms = 0;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.decoder = None;
                    self.ring.clear();
                    self.output = None; // drop the cpal stream
                }
                let _ = self.evt.send(PlayerEvent::Stopped);
            }
            PlayerCommand::Seek { seconds } => {
                let pos_ms = (seconds.max(0.0) * 1000.0) as u64;
                // Reseat position-tracking. ring.clear() (inside seek_to)
                // wipes frames_written; without this update position_ms()
                // would return 0 immediately after every seek.
                self.seek_offset_ms = pos_ms;
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(decoder) = self.decoder.as_mut() {
                    if let Err(e) = decoder.seek_to(seconds) {
                        let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
                    }
                }
                let _ = self.evt.send(PlayerEvent::Progress {
                    position_ms: pos_ms,
                    duration_ms: self.duration_ms,
                    buffered_ms: 0,
                });
            }
            PlayerCommand::SetVolume { amplitude } => {
                self.volume = amplitude.clamp(0.0, 1.0);
                self.ring.set_volume(self.volume);
                let _ = self.evt.send(PlayerEvent::VolumeChanged { amplitude: self.volume });
            }
            PlayerCommand::Next => {
                if let Some(id) = self.queue.next().cloned() {
                    self.play_track(&id);
                } else {
                    self.stop_internal();
                }
            }
            PlayerCommand::Prev => {
                let pos = self.position_ms();
                if pos > 3_000 {
                    self.seek_offset_ms = 0;
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(decoder) = self.decoder.as_mut() {
                        let _ = decoder.seek_to(0.0);
                    }
                } else if let Some(id) = self.queue.prev().cloned() {
                    self.play_track(&id);
                }
            }
            PlayerCommand::Enqueue { track_id } => {
                self.queue.enqueue(track_id);
                self.emit_queue_changed();
            }
            PlayerCommand::EnqueueNext { track_id } => {
                self.queue.enqueue_next(track_id);
                self.emit_queue_changed();
            }
            PlayerCommand::ClearQueue => {
                self.queue.clear();
                self.emit_queue_changed();
            }
            PlayerCommand::RemoveFromQueue { index } => {
                self.queue.remove_at(index);
                self.emit_queue_changed();
            }
            PlayerCommand::MoveInQueue { from, to } => {
                self.queue.move_item(from, to);
                self.emit_queue_changed();
            }
            PlayerCommand::SetShuffle { enabled } => {
                self.queue.set_shuffle(enabled);
            }
            PlayerCommand::SetRepeat { mode } => {
                self.queue.set_repeat(mode);
            }
            PlayerCommand::SetOutputDevice { name } => {
                self.output_device = name;
                let device = self.output_device.clone().unwrap_or_else(|| "<default>".into());
                let _ = self.evt.send(PlayerEvent::OutputDeviceChanged { device_name: device });
            }
            PlayerCommand::SetReplayGain { mode } => {
                self.replay_gain = match mode {
                    ReplayGainCommand::Off => ReplayGainMode::Off,
                    ReplayGainCommand::Track => ReplayGainMode::Track,
                    ReplayGainCommand::Album => ReplayGainMode::Album,
                };
            }
            PlayerCommand::Shutdown => {} // handled in run()
        }
    }

    fn stop_internal(&mut self) {
        self.current_track = None;
        self.duration_ms = 0;
        self.seek_offset_ms = 0;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.decoder = None;
            self.ring.clear();
            self.output = None;
        }
        let _ = self.evt.send(PlayerEvent::Stopped);
    }

    fn play_track(&mut self, track_id: &str) {
        // New track: position counter resets, ring will be cleared.
        self.seek_offset_ms = 0;

        let (track, path) = match self.resolver.resolve(track_id) {
            Ok(v) => v,
            Err(e) => {
                let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
                return;
            }
        };

        let dur = track.duration_ms.unwrap_or(0).max(0) as u64;
        self.duration_ms = dur;

        let gain = linear_gain(
            self.replay_gain,
            GainValues {
                track_db: track.replay_gain_track,
                album_db: track.replay_gain_album,
            },
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Reset decoder + ring for the new track.
            self.decoder = None;
            self.ring.clear();

            // Open the cpal stream lazily on first play. Same stream is
            // reused across tracks because the decoder resamples to the
            // device's rate.
            if self.output.is_none() {
                match NativeOutput::start_default(self.ring.clone()) {
                    Ok(out) => {
                        tracing::info!(
                            rate = out.sample_rate_hz,
                            channels = out.channels,
                            device = %out.device_name,
                            "audio output configured (decoder will resample to this)"
                        );
                        let _ = self.evt.send(PlayerEvent::OutputDeviceChanged {
                            device_name: out.device_name.clone(),
                        });
                        self.output = Some(out);
                    }
                    Err(e) => {
                        let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
                        return;
                    }
                }
            }

            // Tell the ring what the OUTPUT format is so DecodeStream can
            // build its resampler with the correct ratio.
            let (out_rate, out_channels) = self
                .output
                .as_ref()
                .map(|o| (o.sample_rate_hz, o.channels))
                .unwrap_or((44_100, 2));
            self.ring.set_output_format(out_rate, out_channels);

            match DecodeStream::open_file(&path, self.ring.clone(), gain) {
                Ok(decoder) => {
                    // If Symphonia knew the duration, prefer that — tag
                    // duration is sometimes wrong/missing.
                    if let Some(secs) = decoder.duration_secs {
                        self.duration_ms = (secs * 1000.0) as u64;
                    }
                    self.decoder = Some(decoder);
                    self.current_track = Some(track.clone());
                    self.paused = false;
                    let _ = self.evt.send(PlayerEvent::Playing {
                        track,
                        duration_ms: self.duration_ms,
                    });
                }
                Err(e) => {
                    let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
                }
            }
        }

        // wasm path: emit Playing without a real decode pipeline; the web
        // backend would substitute in a future iteration.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            let _ = gain;
            self.current_track = Some(track.clone());
            self.paused = false;
            let _ = self.evt.send(PlayerEvent::Playing {
                track,
                duration_ms: self.duration_ms,
            });
        }
    }

    fn on_track_ended(&mut self) {
        if let Some(t) = &self.current_track {
            let _ = self.evt.send(PlayerEvent::TrackEnded { track_id: t.id.clone() });
        }
        if let Some(next_id) = self.queue.next().cloned() {
            self.play_track(&next_id);
        } else {
            self.stop_internal();
        }
    }

    fn emit_queue_changed(&self) {
        let mut snapshot = Vec::new();
        for id in self.queue.snapshot() {
            if let Ok((t, _)) = self.resolver.resolve(id) {
                snapshot.push(t);
            }
        }
        let _ = self.evt.send(PlayerEvent::QueueChanged { queue: snapshot });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Track;

    struct StubResolver;
    impl TrackResolver for StubResolver {
        fn resolve(&self, track_id: &str) -> Result<(Track, std::path::PathBuf)> {
            // Resolve to a non-existent path. The decoder open will fail,
            // and we expect the engine to emit an Error event rather than
            // crash. This documents the failure mode of unresolvable tracks.
            let t = Track {
                id: track_id.to_string(),
                title: format!("Title {track_id}"),
                artist_id: None,
                album_artist_id: None,
                album_id: None,
                source_id: "src".into(),
                remote_path: format!("/{track_id}.mp3"),
                local_cache_path: None,
                duration_ms: Some(1_000),
                track_number: None,
                disc_number: 1,
                genre: None,
                year: None,
                bpm: None,
                replay_gain_track: None,
                replay_gain_album: None,
                file_size_bytes: None,
                format: Some("mp3".into()),
                bitrate_kbps: None,
                sample_rate_hz: None,
                bit_depth: None,
                channels: None,
                content_hash: None,
                musicbrainz_id: None,
                play_count: 0,
                last_played_at: None,
                rating: None,
                loved: 0,
                created_at: 0,
                updated_at: 0,
            };
            Ok((t, std::path::PathBuf::from("/nonexistent/test/track.mp3")))
        }
    }

    #[test]
    fn engine_emits_error_when_file_not_found() {
        let h = spawn(Arc::new(StubResolver));
        h.send(PlayerCommand::Play { track_id: "t1".into() }).unwrap();

        // Either an Error or a Playing+immediate-error sequence is acceptable;
        // we just check we get *some* event without deadlocking.
        let evt = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2));
        assert!(evt.is_ok(), "engine should emit an event for an unresolvable track");
        h.send(PlayerCommand::Shutdown).unwrap();
    }

    #[test]
    fn engine_handles_shutdown_cleanly() {
        let h = spawn(Arc::new(StubResolver));
        h.send(PlayerCommand::Shutdown).unwrap();
        // Drain whatever event came out (Stopped) and confirm no panic.
        let _ = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2));
    }
}
