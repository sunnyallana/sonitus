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
//!     ringbuffer (lock-free)
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
use crate::player::commands::{PlayerCommand, ReplayGainCommand, RepeatMode};
use crate::player::events::PlayerEvent;
use crate::player::queue::PlayQueue;
use crate::player::replaygain::{GainValues, linear_gain};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::sync::Arc;

/// Cheap clonable handle to the player engine.
///
/// Cloning a handle just clones the underlying channel `Sender` — the
/// decode thread is shared. Drop all clones to let the thread exit.
#[derive(Clone)]
pub struct PlayerHandle {
    cmd_tx: Sender<PlayerCommand>,
    /// Subscribers receive a clone of every event. We use a broadcaster
    /// pattern: a single source channel that we fan-out via `Arc<Mutex>`.
    /// For this MVP-grade engine we expose a single receiver channel.
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
    /// Position within the current track in milliseconds.
    position_ms: u64,
    /// Duration of the current track in milliseconds.
    duration_ms: u64,
    evt: Sender<PlayerEvent>,
    resolver: Arc<dyn TrackResolver>,
    /// Current track being played (for emitting events).
    current_track: Option<Track>,
}

impl EngineState {
    fn new(evt: Sender<PlayerEvent>, resolver: Arc<dyn TrackResolver>) -> Self {
        Self {
            queue: PlayQueue::new(),
            volume: 1.0,
            replay_gain: ReplayGainMode::Track,
            output_device: None,
            paused: false,
            position_ms: 0,
            duration_ms: 0,
            evt,
            resolver,
            current_track: None,
        }
    }

    fn run(&mut self, cmd_rx: Receiver<PlayerCommand>) {
        // The actual decode loop would integrate Symphonia + cpal here.
        // For now we drive the state machine and emit events; a real audio
        // backend is plugged in via `output_native` / `output_web`.
        loop {
            // Block on next command. The decode thread itself, when active,
            // would also tick a decode cycle; we use a select! pattern:
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(cmd) => {
                    if matches!(cmd, PlayerCommand::Shutdown) {
                        let _ = self.evt.send(PlayerEvent::Stopped);
                        return;
                    }
                    self.handle_command(cmd);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Tick: emit progress if playing.
                    if !self.paused && self.current_track.is_some() {
                        self.position_ms = (self.position_ms + 100).min(self.duration_ms);
                        let _ = self.evt.send(PlayerEvent::Progress {
                            position_ms: self.position_ms,
                            duration_ms: self.duration_ms,
                            buffered_ms: self.duration_ms,
                        });
                        if self.position_ms >= self.duration_ms && self.duration_ms > 0 {
                            self.on_track_ended();
                        }
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn handle_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::Play { track_id } => self.play_track(&track_id),
            PlayerCommand::PlayUrl { url: _ } => {
                // Simplified: HTTP source streaming wires through the source
                // provider. For the engine state machine we emit a stub event.
                let _ = self.evt.send(PlayerEvent::Error {
                    message: "PlayUrl not yet implemented in this engine build".into(),
                });
            }
            PlayerCommand::Pause => {
                self.paused = true;
                let _ = self.evt.send(PlayerEvent::Paused { position_ms: self.position_ms });
            }
            PlayerCommand::Resume => {
                self.paused = false;
                let _ = self.evt.send(PlayerEvent::Resumed { position_ms: self.position_ms });
            }
            PlayerCommand::Stop => {
                self.paused = false;
                self.position_ms = 0;
                self.current_track = None;
                let _ = self.evt.send(PlayerEvent::Stopped);
            }
            PlayerCommand::Seek { seconds } => {
                self.position_ms = (seconds.max(0.0) * 1000.0) as u64;
                let _ = self.evt.send(PlayerEvent::Progress {
                    position_ms: self.position_ms,
                    duration_ms: self.duration_ms,
                    buffered_ms: self.duration_ms,
                });
            }
            PlayerCommand::SetVolume { amplitude } => {
                self.volume = amplitude.clamp(0.0, 1.0);
                let _ = self.evt.send(PlayerEvent::VolumeChanged { amplitude: self.volume });
            }
            PlayerCommand::Next => {
                if let Some(id) = self.queue.next().cloned() {
                    self.play_track(&id);
                } else {
                    let _ = self.evt.send(PlayerEvent::Stopped);
                    self.current_track = None;
                }
            }
            PlayerCommand::Prev => {
                if self.position_ms > 3_000 {
                    self.position_ms = 0;
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

    fn play_track(&mut self, track_id: &str) {
        match self.resolver.resolve(track_id) {
            Ok((track, _path)) => {
                let dur = track.duration_ms.unwrap_or(0).max(0) as u64;
                self.position_ms = 0;
                self.duration_ms = dur;
                let _gain = linear_gain(
                    self.replay_gain,
                    GainValues {
                        track_db: track.replay_gain_track,
                        album_db: track.replay_gain_album,
                    },
                );
                // The real decode pipeline would seed the symphonia reader
                // and start filling the ringbuffer here.
                self.current_track = Some(track.clone());
                let _ = self.evt.send(PlayerEvent::Playing { track, duration_ms: dur });
            }
            Err(e) => {
                let _ = self.evt.send(PlayerEvent::Error { message: e.to_string() });
            }
        }
    }

    fn on_track_ended(&mut self) {
        if let Some(t) = &self.current_track {
            let _ = self.evt.send(PlayerEvent::TrackEnded { track_id: t.id.clone() });
        }
        if let Some(next_id) = self.queue.next().cloned() {
            self.play_track(&next_id);
        } else {
            self.current_track = None;
            self.duration_ms = 0;
            self.position_ms = 0;
            let _ = self.evt.send(PlayerEvent::Stopped);
        }
    }

    fn emit_queue_changed(&self) {
        // Resolve each queued ID into a Track for the UI snapshot.
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
            Ok((t, std::path::PathBuf::from("/tmp/x.mp3")))
        }
    }

    #[test]
    fn engine_emits_playing_event_on_play_command() {
        let h = spawn(Arc::new(StubResolver));
        h.send(PlayerCommand::Play { track_id: "t1".into() }).unwrap();

        // Wait briefly for the event.
        let evt = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        match evt {
            PlayerEvent::Playing { track, duration_ms } => {
                assert_eq!(track.id, "t1");
                assert_eq!(duration_ms, 1_000);
            }
            other => panic!("expected Playing, got {other:?}"),
        }

        h.send(PlayerCommand::Shutdown).unwrap();
    }

    #[test]
    fn pause_and_resume_emit_corresponding_events() {
        let h = spawn(Arc::new(StubResolver));
        h.send(PlayerCommand::Play { track_id: "t1".into() }).unwrap();
        // Drain the Playing event.
        let _ = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2)).unwrap();

        h.send(PlayerCommand::Pause).unwrap();
        loop {
            let e = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            if matches!(e, PlayerEvent::Paused { .. }) { break; }
        }
        h.send(PlayerCommand::Resume).unwrap();
        loop {
            let e = h.event_receiver().recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            if matches!(e, PlayerEvent::Resumed { .. }) { break; }
        }
        h.send(PlayerCommand::Shutdown).unwrap();
    }
}
